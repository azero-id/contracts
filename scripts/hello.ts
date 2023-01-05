import {ApiPromise, Keyring, WsProvider} from "@polkadot/api";
import {MerkleTree} from "merkletreejs";
import {BlueprintPromise, ContractPromise} from '@polkadot/api-contract'
import abi from "../merkle_verifier/target/ink/metadata.json";
import {CodePromise} from '@polkadot/api-contract';
import {readFileSync} from "fs";

const SHA256 = require('crypto-js/sha256')
import {keccak256} from 'js-sha3';

const wasm = readFileSync('/Users/vacekj/Programming/contracts/merkle_verifier/target/ink/merkle_verifier.wasm');

async function main() {
    const leaves = ['a', 'b', 'c'].map(x => SHA256(x))
    const tree = new MerkleTree(leaves, keccak256, {
        sortPairs: true
    })
    const root = tree.getRoot();
    const leaf = SHA256('a')
    const proof = tree.getHexProof(leaf)

    const badLeaves = ['a', 'x', 'c'].map(x => SHA256(x))
    const badTree = new MerkleTree(badLeaves, SHA256)
    const badLeaf = SHA256('x')
    const badProof = badTree.getProof(badLeaf)

    const provider = new WsProvider('ws://127.0.0.1:9944')
    const api = await ApiPromise.create({provider})

    const code = new CodePromise(api, abi, wasm);
    let keyring = new Keyring();
    const alicePair = keyring.addFromUri('//Alice', {name: 'Alice default'});
    console.log('alice address ', alicePair.address);
    console.log('root ', tree.getHexRoot());

    // maximum gas to be consumed for the instantiation. if limit is too small the instantiation will fail.
    const gasLimit = 100000n * 1000000n
    // a limit to how much Balance to be used to pay for the storage created by the instantiation
    // if null is passed, unlimited balance can be used
    const storageDepositLimit = null

    const tx = code.tx.new({gasLimit, storageDepositLimit}, root)

    let address;
    const unsub = await tx.signAndSend(alicePair, ({contract, status}: any) => {
        if (status.isInBlock || status.isFinalized) {
            address = contract.address.toString();
            console.log(address)
            unsub();
        }
    });
    await new Promise((resolve) => setTimeout(() => {
        resolve(0);
    }, 1000));

    const contract = new ContractPromise(api, abi, address);

    //let {gasRequired} = await contract.query.root(alicePair.address, {storageDepositLimit: null, gasLimit: -1},);
    // @ts-ignore
    //const newGasLimit = gasRequired * 1.5
    let res = await contract.query.root();

    console.log(res);
}

main().catch((error) => {
    console.error(error);
    process.exit(-1);
}).finally(() => {
    process.exit(-1);
});
