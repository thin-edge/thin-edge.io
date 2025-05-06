import { componentize } from '@bytecodealliance/componentize-js';
import { readFile, writeFile } from 'node:fs/promises';

const jsSource = await readFile('src/hello.js', 'utf8');
const witSource = await readFile('src/hello.wit', 'utf8');
const disableFeatures = ['stdio', 'http', 'clocks', 'random']; 
const options = {
	disableFeatures,
	debugBuild: false
};


const { component } = await componentize(jsSource, witSource, options);

await writeFile('build/hello.wasm', component );
