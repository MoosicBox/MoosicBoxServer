/// <reference path="./.sst/platform/config.d.ts" />
import { readdirSync } from 'fs';
export default $config({
    app(input) {
        return {
            name: 'moosicbox',
            removal: input?.stage === 'prod' ? 'retain' : 'remove',
            home: 'aws',
            providers: {
                digitalocean: '4.31.1',
                kubernetes: '4.17.1',
                awsx: '2.14.0',
            },
        };
    },
    async run() {
        const outputs = {};
        const result = await import(`./infra/tunnel-server.ts`);
        Object.assign(outputs, result.outputs);
        // for (const value of readdirSync('./infra/')) {
        //     const result = await import(`./infra/${value}`);
        //     if (result.outputs) {
        //         Object.assign(outputs, result.outputs);
        //     }
        // }
        return outputs;
    },
});
