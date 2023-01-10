import { ApiPromise, Keyring } from "@polkadot/api";
import { CodePromise, ContractPromise } from "@polkadot/api-contract";
import { IKeyringPair } from "@polkadot/types/types/interfaces";
import BN from "bn.js";
import { SHA256 } from "crypto-js";
import { readFileSync } from "fs";
import { keccak256 } from "js-sha3";
import { MerkleTree } from "merkletreejs";
import merkleVerifierABI from "../target/ink/merkle_verifier/metadata.json";
import aznsRegistryABI from "../target/ink/azns_registry/metadata.json";
import { bufferToU8a, hexToU8a } from '@polkadot/util';

const merkleVerifierWASM = readFileSync("./target/ink/merkle_verifier/merkle_verifier.wasm");
const aznsRegisterWASM = readFileSync("./target/ink/azns_registry/azns_registry.wasm");

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
 * Constructs the merkle tree from the whitelisted accounts
 * @returns MerkleTree Object
 */
const constructMerkleTree = async () => {
  const leaves = ["a", "b", "c", keyring.decodeAddress(alice.address)].map((x) => SHA256(x));
  const tree = new MerkleTree(leaves, keccak256, {
    sortPairs: true,
  });

  return tree;
}

/**
 * Helper function to deploy a contract
 * @dev Assumes constructor name to be `new`
 * @param signer Account which will deploy the contract
 * @param abi Metadata of the contract
 * @param wasm WASM object of the contract
 * @param params Parameters used to init contract
 * @returns (contractAddress, codeHash) of the deployed contract
 */
const deployContract = async (signer: IKeyringPair, abi, wasm, params) => {
  const code = new CodePromise(api, abi, wasm);

  let contractAddress, codeHash;
  await code.tx
    .new(
      { gasLimit, storageDepositLimit: null },
      ...params
    )
    .signAndSend(signer, ({ contract, status }: any) => {
      if (status?.isInBlock || status?.isFinalized) {
        contractAddress = contract.address.toString();
        codeHash = contract.abi.json.source.hash;
        console.log("Contract deployed at %s with code hash (%s)", contractAddress, codeHash);
      }
    });

  // HACK: Only way to keep callback/closure alive
  await new Promise((r) => setTimeout(r, 1000));

  return { contractAddress, codeHash };
}

/**
 * Deploys `azns_registry` contract
 * @param signer Account which will deploy the contract
 * @param tree MerkleTree object to instantiate contract with
 * @returns Address of deployed contract
 */
const instantiate = async (
  signer: IKeyringPair,
  tree: MerkleTree
): Promise<{ contractAddress: string }> => {

  // Convert MerkleTree root to Uint8Array
  const root_encoded = bufferToU8a(tree.getRoot());

  // Deploy `merkle_verifier` contract and obtain its codeHash
  const { codeHash } = await deployContract(signer, merkleVerifierABI, merkleVerifierWASM, [root_encoded]);

  // Deploy `azns_registry` contract
  const { contractAddress } = await deployContract(signer, aznsRegistryABI, aznsRegisterWASM, [null, codeHash, root_encoded, null]);

  return { contractAddress };
};

/**
 * 
 * @param tree MerkleTree object on which the proof is constructed
 * @param accountId Item whose inclusion needs to be proved
 * @returns Buffer[] - proof of the inclusion of given accountId in the merkle tree
 */
const generateProof = async (tree: MerkleTree, accountId: string) => {
  const leaf = SHA256(keyring.decodeAddress(accountId));
  const proof = tree.getProof(leaf).map(x => x.data);

  console.log("Off-chain verification:", tree.verify(proof, leaf, tree.getRoot()));
  return proof;
}

/**
 * Registers a domain by a whitelisted account
 * @param contractAddress `azns_registry` deployed address
 * @param signer Account which will sign the tx
 * @param domain Domain name to be registered
 * @param price The price user is willing to pay
 * @param proof proof of whitelist of given accountId
 */
const register_with_proof = async (contractAddress: string, signer: IKeyringPair, domain: string, price: BN, proof: Buffer[]) => {
  const contract = new ContractPromise(api, aznsRegistryABI, contractAddress);

  let { result, output } = await contract.query.verifyProof(signer.address, {
    gasLimit,
  }, signer.address, proof);

  if (result.isOk && !!output) {
    console.log("On-chain verification:", output.toJSON());

    if (output.toPrimitive()["ok"]) {
      console.log("verifyProof works.");
    } else {
      console.log("On-chain proof failed.");
    }
  } else {
    console.error("Error while querying contract. Got result:", result);
  }
  return;

  // Register domain with proof
  await contract.tx.register({
    value: price,
    gasLimit,
  }, domain, proof).signAndSend(signer, result => {
    if (result.status.isFinalized) {
      console.log('finalized');
    }
  });
  // HACK: Only way to keep callback/closure alive
  await new Promise((r) => setTimeout(r, 1000));
}

async function test() {
  await initPolkadotJs();

  // 1. Construct merkle tree
  const tree = await constructMerkleTree();

  // 2. Deploy AZNS-Registry contract
  const { contractAddress } = await instantiate(alice, tree);

  // 3. Generate proof of a whitelisted address
  const proof = await generateProof(tree, alice.address);

  // 4. Register a domain during whitelisted phase
  const domain = "AZNS";
  const price = new BN(1_000_000);
  await register_with_proof(contractAddress, alice, domain, price, proof);
}

test()
  .catch((error) => {
    console.error(error);
  })
  .finally(() => {
    process.exit(-1);
  });
