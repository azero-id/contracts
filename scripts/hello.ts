import {ApiPromise} from "@polkadot/api";
import {MerkleTree} from "merkletreejs";
import {ContractPromise} from '@polkadot/api-contract'
import abi from "../merkle_verifier/target/ink/metadata.json";

const SHA256 = require('crypto-js/sha256')
import {keccak256} from 'js-sha3';

async function main() {
    const leaves = ['a', 'b', 'c'].map(x => SHA256(x))
    const tree = new MerkleTree(leaves, keccak256, {
        sortPairs: true
    })
    const root = tree.getRoot().toString('hex')
    const leaf = SHA256('a')
    const proof = tree.getHexProof(leaf)
    console.log(tree.verify(proof, leaf, root)) // true
    console.log(keccak256('hello'));

    const badLeaves = ['a', 'x', 'c'].map(x => SHA256(x))
    const badTree = new MerkleTree(badLeaves, SHA256)
    const badLeaf = SHA256('x')
    const badProof = badTree.getProof(badLeaf)
    console.log(badTree.verify(badProof, badLeaf, root)) // false

    // Create our API with a default connection to the local node
    const api = await ApiPromise.create();
    // const contract = new ContractPromise(api, abi,)
}

main().catch((error) => {
    console.error(error);
    process.exit(-1);
}).finally(() => {
    process.exit(-1);
});
