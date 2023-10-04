import { SSMClient } from '@aws-sdk/client-ssm';
import { StackContext, Api, WebSocketApi } from 'sst/constructs';
import { fetchSstSecret } from '../sst-secrets';

export async function API({ app, stack }: StackContext) {
    const ssm = new SSMClient({ region: stack.region });

    const api = new Api(stack, 'api', {
        defaults: {
            function: {
                runtime: 'rust',
                timeout: '5 minutes',
                environment: {
                    PROXY_HOST: await fetchSstSecret(
                        ssm,
                        app.name,
                        'PROXY_HOST',
                        app.stage,
                    ),
                },
            },
        },
        routes: {
            'GET /albums': 'packages/menu/src/moosicbox_menu.handler',
            'GET /track': 'packages/files/src/moosicbox_files.handler',
            'GET /ws': 'packages/ws/src/moosicbox_ws.handler',
        },
    });

    const websocketApi = new WebSocketApi(stack, 'websockets', {
        defaults: {
            function: {
                runtime: 'rust',
                timeout: '5 minutes',
                environment: {
                    PROXY_HOST: await fetchSstSecret(
                        ssm,
                        app.name,
                        'PROXY_HOST',
                        app.stage,
                    ),
                },
            },
        },
        routes: {
            // $connect: 'packages/connections/src/moosicbox_connections.handler',
            // $default: 'packages/connections/src/moosicbox_connections.handler',
            // $disconnect:
            //     'packages/connections/src/moosicbox_connections.handler',
            // sendMessage:
            //     'packages/connections/src/moosicbox_connections.handler',
            $connect: 'packages/ws/src/moosicbox_ws.handler',
            $default: 'packages/ws/src/moosicbox_ws.handler',
            $disconnect:
                'packages/ws/src/moosicbox_ws.handler',
            sendMessage:
                'packages/ws/src/moosicbox_ws.handler',
        },
    });

    stack.addOutputs({
        ApiEndpoint: api.url,
        WebsocketApiEndpoint: websocketApi.url,
    });
}
