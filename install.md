  # 1. Compiler le LSP                                                                                                                  
  cd /Users/plawn/Documents/konf-provider/konf-lsp                                                                                      
  cargo +nightly build --release                                                                                                        
                                                                                                                                        
  # 2. Créer le package                                                                                                                 
  cd vscode-extension                                                                                                                   
  bun install                                                                                                                           
  bun run compile                                                                                                                       
  bun run package   # Crée konf-provider-0.1.0.vsix                                                                                     
                                                                                                                                        
  Puis dans VSCode:                                                                                                                     
  - Cmd+Shift+P → "Extensions: Install from VSIX..."                                                                                    
  - Sélectionner le fichier .vsix généré                                                                                                
                                                                                                                                        
  Option 3: Lien symbolique (dev rapide)                                                                                                
                                                                                                                                        
  # Lier l'extension dans le dossier extensions de VSCode                                                                               
  ln -s /Users/plawn/Documents/konf-provider/konf-lsp/vscode-extension ~/.vscode/extensions/konf-provider                               
                                                                                                                                        
  Puis redémarrer VSCode.                                                                                                               
                                                                                                                                        
  ---                                                                                                                                   
  Note: Pour que le LSP fonctionne, assure-toi que le binaire konf-lsp est accessible. Tu peux soit:                                    
  - Le mettre dans ton PATH                                                                                                             
  - Configurer konf.serverPath dans les settings VSCode