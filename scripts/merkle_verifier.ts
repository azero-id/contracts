import { ApiPromise, Keyring } from "@polkadot/api";
import { CodePromise, ContractPromise } from "@polkadot/api-contract";
import { IKeyringPair } from "@polkadot/types/types/interfaces";
import BN from "bn.js";
import { SHA256 } from "crypto-js";
import { readFileSync } from "fs";
import { keccak256 } from "js-sha3";
import { MerkleTree } from "merkletreejs";
import abi from "../target/ink/merkle_verifier/metadata.json";
import { bufferToU8a, hexToU8a } from '@polkadot/util';

const wasm = readFileSync("./target/ink/merkle_verifier/merkle_verifier.wasm");

let api: ApiPromise;
let keyring: Keyring;
let alice: IKeyringPair;
let gasLimit: any;

/**
 * Initialized polkadot.js & development keypair
 */
const initPolkadotJs = async () => {
  api = await ApiPromise.create();
  keyring = new Keyring({ type: "sr25519" });
  alice = keyring.createFromUri("//Alice");

  // HACK: Workaround for polkadot.js gasLimit incompatibility with WeightsV2 (https://github.com/polkadot-js/api/issues/5255)
  gasLimit = api.registry.createType("WeightV2", {
    refTime: new BN("100000000000"),
    proofSize: new BN("10000000000"),
  });
};

/**
 * Deploys `merkle_verifier` contract with root of given MerkleTree and
 *   verifies that it got stored correctly.
 * @param tree MerkleTree object to instantiate contract with
 * @returns Address of deployed contract
 */
const instantiate = async (
  tree: MerkleTree
): Promise<{ contractAddress: string }> => {
  // Convert MerkleTree root to Uint8Array
  const rootBuffer = tree.getRoot();
  const rootUint8Array = bufferToU8a(rootBuffer);

  // Deploy contract
  const code = new CodePromise(api, abi, wasm);
  let contractAddress;
  await code.tx
    .new(
      { gasLimit: 1000n * 1000000n, storageDepositLimit: null },
      rootUint8Array
    )
    .signAndSend(alice, ({ contract, status }: any) => {
      if (status?.isInBlock || status?.isFinalized) {
        contractAddress = contract.address.toString();
        console.log("Contract deployed at:", contractAddress);
      }
    });
  // HACK: Only way to keep callback/closure alive
  await new Promise((r) => setTimeout(r, 1000));

  // Query contract for root
  const contract = new ContractPromise(api, abi, contractAddress);
  const { result, output } = await contract.query.root(alice.address, {
    gasLimit,
  });

  if (result.isOk && !!output) {
    let valueU8a = output.toU8a();
    valueU8a = valueU8a.slice(1, valueU8a.length);
    const arraysAreEqual =
      JSON.stringify(valueU8a) === JSON.stringify(rootUint8Array);
    if (!arraysAreEqual)
      console.error(
        "On-chain Uint8Array does not equal MerkleTree root:",
        rootUint8Array,
        valueU8a
      );
    else
      console.log(
        "Successfully stored & queries MerkleTree root from contract."
      );
  } else {
    console.error("Error while querying contract. Got result:", result);
  }

  return { contractAddress };
};

/**
 * Checks on-chain Proofs work
 * @param tree MerkleTree object on which the proof is constructed
 * @param contractAddress MerkleVerifier contract address
 * @param item Item whose inclusion needs to be verifed
 */
const verifyProofs = async (tree: MerkleTree, contractAddress: string, item: string) => {
  const leaf = SHA256(item);
  const leaf_encoded = hexToU8a(leaf.toString());
  const proof = tree.getProof(leaf).map(x => x.data);

  const contract = new ContractPromise(api, abi, contractAddress);
  const { result, output } = await contract.query.verifyProof(alice.address, {
    gasLimit,
  }, leaf_encoded, proof);

  if (result.isOk && !!output) {
    let res = output.toPrimitive()["ok"];

    if (res) {
      console.log("verifyProof works.");
    } else {
      console.log("On-chain proof failed.");
    }
  } else {
    console.error("Error while querying contract. Got result:", result);
  }
};

async function main() {
  await initPolkadotJs();

  // Construct MerkleTree
  const leaves = ["a", "b", "c"].map((x) => SHA256(x));
  const tree = new MerkleTree(leaves, keccak256, {
    sortPairs: true,
  });

  const { contractAddress } = await instantiate(tree);

  // TODO
  const leaf = SHA256("a");
  const proof = tree.getHexProof(leaf);
  const badLeaves = ["a", "x", "c"].map((x) => SHA256(x));
  const badTree = new MerkleTree(badLeaves, SHA256);
  const badLeaf = SHA256("x");
  const badProof = badTree.getProof(badLeaf);

  await verifyProofs(tree, contractAddress, "b");
}

main()
  .catch((error) => {
    console.error(error);
  })
  .finally(() => {
    process.exit(-1);
  });
