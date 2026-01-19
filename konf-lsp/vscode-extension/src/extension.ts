import * as path from 'path';
import * as fs from 'fs';
import { workspace, ExtensionContext, window } from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: ExtensionContext) {
    // Find the language server binary
    const serverPath = findServerPath(context);

    if (!serverPath) {
        window.showErrorMessage(
            'Konf LSP: Could not find the konf-lsp binary. Please set konf.serverPath in settings or ensure konf-lsp is in your PATH.'
        );
        return;
    }

    // Server options - run the LSP binary
    const serverOptions: ServerOptions = {
        run: {
            command: serverPath,
            transport: TransportKind.stdio,
        },
        debug: {
            command: serverPath,
            transport: TransportKind.stdio,
            options: {
                env: {
                    ...process.env,
                    RUST_LOG: 'konf_lsp=debug',
                },
            },
        },
    };

    // Client options
    const clientOptions: LanguageClientOptions = {
        // Register for YAML files
        documentSelector: [
            { scheme: 'file', language: 'yaml' },
            { scheme: 'file', language: 'konf-yaml' },
            { scheme: 'file', pattern: '**/*.yaml' },
            { scheme: 'file', pattern: '**/*.yml' },
        ],
        synchronize: {
            // Watch for YAML file changes
            fileEvents: workspace.createFileSystemWatcher('**/*.{yaml,yml}'),
        },
    };

    // Create the language client
    client = new LanguageClient(
        'konf-lsp',
        'Konf Provider Language Server',
        serverOptions,
        clientOptions
    );

    // Start the client (also starts the server)
    client.start();

    console.log(`Konf Provider extension activated, using server: ${serverPath}`);
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}

/**
 * Find the konf-lsp binary
 */
function findServerPath(context: ExtensionContext): string | undefined {
    // 1. Check user configuration
    const config = workspace.getConfiguration('konf');
    const configuredPath = config.get<string>('serverPath');
    if (configuredPath && fs.existsSync(configuredPath)) {
        return configuredPath;
    }

    // 2. Check bundled binary in extension
    const bundledPath = context.asAbsolutePath(
        path.join('bin', process.platform === 'win32' ? 'konf-lsp.exe' : 'konf-lsp')
    );
    if (fs.existsSync(bundledPath)) {
        return bundledPath;
    }

    // 3. Check PATH
    const pathDirs = (process.env.PATH || '').split(path.delimiter);
    for (const dir of pathDirs) {
        const candidate = path.join(
            dir,
            process.platform === 'win32' ? 'konf-lsp.exe' : 'konf-lsp'
        );
        if (fs.existsSync(candidate)) {
            return candidate;
        }
    }

    // 4. Check development locations relative to extension
    // Extension is at: konf-lsp/vscode-extension
    // Binary is at: konf-lsp/target/release/konf-lsp
    const devPaths = [
        // Relative to extension directory (konf-lsp/vscode-extension -> konf-lsp/target)
        path.join(context.extensionPath, '..', 'target', 'release', 'konf-lsp'),
        path.join(context.extensionPath, '..', 'target', 'debug', 'konf-lsp'),
        // Relative to workspace
        path.join(process.cwd(), 'konf-lsp', 'target', 'release', 'konf-lsp'),
        path.join(process.cwd(), 'konf-lsp', 'target', 'debug', 'konf-lsp'),
        path.join(process.cwd(), 'target', 'release', 'konf-lsp'),
        path.join(process.cwd(), 'target', 'debug', 'konf-lsp'),
    ];

    for (const devPath of devPaths) {
        const fullPath = process.platform === 'win32' ? devPath + '.exe' : devPath;
        if (fs.existsSync(fullPath)) {
            return fullPath;
        }
    }

    return undefined;
}
