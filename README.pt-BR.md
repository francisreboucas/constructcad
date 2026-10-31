# ConstructCAD

[![CI](https://github.com/francisreboucas/constructcad/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/francisreboucas/constructcad/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/licen%C3%A7a-GPL--3.0--or--later-blue.svg)](LICENSE)
[![Rust 2024](https://img.shields.io/badge/rust-2024_edition-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![IFC4](https://img.shields.io/badge/IFC-exporta_IFC4_%C2%B7_importa_IFC2X3%2FIFC4-0a7bbb.svg)](https://technical.buildingsmart.org/standards/ifc/)
[![Platforms](https://img.shields.io/badge/plataformas-Windows_%C2%B7_macOS_%C2%B7_Linux-lightgrey.svg)](#executar)
[![DCO](https://img.shields.io/badge/DCO-commits_assinados-brightgreen.svg)](CONTRIBUTING.md)

Software livre de **Modelagem de Informações da Construção (BIM)**, escrito em Rust e independente de qualquer fornecedor comercial.

[English](README.md) · [Como contribuir](CONTRIBUTING.md) · [Código de conduta](CODE_OF_CONDUCT.md) · [Segurança](SECURITY.md)

Um único modelo paramétrico é a fonte da verdade. Plantas, forros, cortes, elevações, vistas 3D, legendas, tabelas e folhas são *consultas* sobre esse modelo e se atualizam automaticamente a cada alteração.

## Familiar por design

Os menus, atalhos, painéis e ferramentas que suas mãos já conhecem. Produtivo desde o primeiro dia, sem reaprender.

O ConstructCAD segue as convenções comuns às ferramentas CAD e BIM de desktop: barra de acesso rápido no canto superior esquerdo, ribbon com abas, paleta de propriedades e navegador de projeto encaixados na lateral, e atalhos de teclado padrão e de duas letras. São convenções gerais; o ConstructCAD não copia ícones, arte ou textos de nenhum produto (veja [Independência](#independência)).

A dica de cada botão mostra o atalho, o que ele faz, por que está indisponível quando aparece acinzentado e o comando do motor por trás dele. **Ctrl+Shift+P** abre uma paleta que encontra qualquer comando, vista ou comando do motor pelo nome; **Ctrl+F1** recolhe o ribbon até as abas. Veja o [guia da interface](docs/interface.pt-BR.md).

## Capturas de tela

![Planta e vista 3D da casa de exemplo: planta em L com uma parede em arco sob um telhado de quatro águas](docs/screenshots/pt-BR/hero.png)

A casa de exemplo, construída por [`examples/showcase.ccmacro`](examples/showcase.ccmacro) só com comandos do motor e salva em [`examples/showcase.ccad`](examples/showcase.ccad); as imagens são capturadas do aplicativo por [`scripts/screenshots.ps1`](scripts/screenshots.ps1).

| | |
|---|---|
| [![Vista 3D do telhado sobre a parede em arco](docs/screenshots/pt-BR/roof-3d.png)](docs/screenshots/pt-BR/roof-3d.png) | [![Planta hidrossanitária](docs/screenshots/pt-BR/plumbing.png)](docs/screenshots/pt-BR/plumbing.png) |
| Telhado de quatro águas calculado pelo esqueleto reto sobre a parede em arco | Planta hidrossanitária: água fria e esgoto sobre a arquitetura esmaecida, com diâmetros e declividades |
| [![Planta elétrica e diagrama do quadro](docs/screenshots/pt-BR/electrical.png)](docs/screenshots/pt-BR/electrical.png) | [![Planta de forma e vista das armaduras](docs/screenshots/pt-BR/structure.png)](docs/screenshots/pt-BR/structure.png) |
| Planta elétrica com circuitos e eletrodutos, ao lado do diagrama unifilar e do quadro de cargas | Planta de forma com pilares, vigas e sapatas identificados, e as armaduras em 3D |
| [![Planta de reforma e planta filtrada](docs/screenshots/pt-BR/graphics.png)](docs/screenshots/pt-BR/graphics.png) | [![Corte](docs/screenshots/pt-BR/section.png)](docs/screenshots/pt-BR/section.png) |
| Fases de obra (existente, demolido, novo) e filtros de vista colorindo as paredes pela espessura | Corte com hachura, níveis e o telhado |
| [![Folha](docs/screenshots/pt-BR/sheet.png)](docs/screenshots/pt-BR/sheet.png) | [![Tabela de ambientes](docs/screenshots/pt-BR/schedule.png)](docs/screenshots/pt-BR/schedule.png) |
| Folha A3 com a planta, um corte e a vista 3D | Tabela de ambientes com totais |

## Instalar

Cada [versão publicada](https://github.com/francisreboucas/constructcad/releases) traz instaladores, arquivos compactados e um `SHA256SUMS.txt` para conferi-los:

| Sistema | Instalador | Arquivo compactado |
|---|---|---|
| Windows (x64) | `constructcad-vX.Y.Z-x86_64.msi`: instala em Arquivos de Programas com atalho no menu Iniciar, abre arquivos `.ccad` e, opcionalmente, põe o `ccad` no PATH. Versões novas atualizam a instalação existente. | `…-x86_64-pc-windows-msvc.zip` |
| Linux (x86_64, arm64) | `constructcad_X.Y.Z_amd64.deb` ou `…_arm64.deb` (Debian, Ubuntu: `sudo apt install ./constructcad_X.Y.Z_amd64.deb`) ou `constructcad-vX.Y.Z-x86_64.AppImage` ou `…-aarch64.AppImage` (dê `chmod +x` e execute). Ambos trazem o atalho e o ícone; o `.deb` também registra o tipo de arquivo `.ccad` e instala o `ccad` em `/usr/bin`. | `…-x86_64-unknown-linux-gnu.tar.gz`, `…-aarch64-unknown-linux-gnu.tar.gz` |
| macOS | `constructcad-vX.Y.Z-aarch64-apple-darwin.dmg` (Apple silicon) ou `…-x86_64-apple-darwin.dmg` (Intel): arraste o *ConstructCAD* para *Aplicativos*. | `…-apple-darwin.tar.gz` (com o `ccad`) |

As compilações ainda não são assinadas. No Windows, o SmartScreen pode avisar sobre editor desconhecido: escolha *Mais informações → Executar assim mesmo*. No macOS, o Gatekeeper bloqueia a primeira abertura: clique com o botão direito (Control-clique) no aplicativo e escolha *Abrir* ou, no macOS 15 e posteriores, tente abrir uma vez e clique em *Abrir Mesmo Assim* em *Ajustes do Sistema → Privacidade e Segurança*. As versões para Linux pedem glibc 2.35 ou posterior (Ubuntu 22.04, Debian 12 e mais novos) e driver Vulkan ou OpenGL.

## Executar

```sh
cargo run --release -p cc_app                                    # projeto novo
cargo run --release -p cc_app -- examples/casa_simples.ccad      # abrir um projeto
cargo run --release -p cc_app -- examples/casa_simples.ccad --view "Elevação: Sul"
cargo run --release -p cc_app -- --lang en                       # interface em inglês
cargo run -p cc_core --example casa_simples                      # regenerar o exemplo
```

### Aviso de atualização

Ao iniciar (no máximo uma vez por dia) o aplicativo consulta a API do GitHub pela última versão e, se ela for mais nova, mostra um aviso com o link da sua página; ele nunca baixa nada. Desligue em *Arquivo → Sobre* ou com `CONSTRUCTCAD_NO_UPDATE_CHECK=1`; veja [o guia da interface](docs/interface.pt-BR.md#aviso-de-atualização) sobre o que é contatado. `ccad --version` mostra a versão.

## Idiomas

A interface está disponível em **português (Brasil)**, **inglês**, **espanhol**, **francês**, **alemão**, **italiano**, **laosiano**, **polonês**, **ucraniano**, **chinês tradicional** e **chinês simplificado**, com troca em *Arquivo → Idioma*. A escolha fica salva entre sessões. Na primeira execução vale o idioma do sistema, e `--lang en|pt-BR|es|fr|de|it|lo|pl|uk-UA|zh-TW|zh-CN` força um idioma.

Projetos novos recebem os nomes de níveis, materiais, tipos e vistas no idioma ativo. Projetos existentes mantêm seus nomes, pois eles fazem parte dos dados. O separador decimal acompanha o idioma (vírgula ou ponto).

As traduções ficam em `assets/i18n/*.ftl`. Um teste garante que todos os idiomas têm as mesmas chaves e os mesmos placeholders. Para adicionar um idioma, crie o `.ftl` e registre-o em `Language` (`crates/cc_app/src/i18n.rs`).

## Funcionalidades (MVP)

- **Navegador de projeto**: Plantas de piso, Plantas de forro, Vistas 3D, Elevações (N/S/L/O), Cortes, Legendas, Tabelas e Folhas. Pelo menu de contexto é possível abrir, renomear, duplicar, excluir e inserir na folha ativa.
- **Modelagem em planta**:
  - Paredes em cadeia, com junções automáticas em L (meia-esquadria) e T.
  - Portas e janelas hospedadas na parede, com abertura recortada automaticamente.
  - Pisos, forros e telhados (plano ou de uma água, com beiral) por esboço ou "Das paredes".
- **Estrutura e circulação**: pilares (retangulares ou circulares, topo preso a um nível), vigas (as pontas param nas faces dos pilares estruturais e encontram outras vigas em meia-esquadria e de topo; justificação pelo centro ou pelas faces), pilares nos cruzamentos dos eixos e vigas ao longo dos eixos ou entre pilares de uma só vez, tabelas de pilares e vigas com seção, comprimento de corte, volume e material, perfis metálicos e tubulares (I, U, L, tubos), vigas inclinadas, fundações (sapatas, baldrames, radier; `IfcFooting`), lajes unidas aos apoios, modelo analítico (vista 3D e exportação JSON), armaduras com tabela de aço (`IfcReinforcingBar`), contraventamentos e treliças, plantas de forma com etiquetas de seção, escadas retas (espelhos a partir do espelho máximo do tipo) e guarda-corpos; aba *Estrutura*; exportados como `IfcColumn`, `IfcBeam`, `IfcStair`, `IfcRailing`.
- **Telhados**: plano, uma água, duas águas e quatro águas (a menor das águas que sobem de cada beiral), com cumeeira e espigões na planta; telhados inclinados saem no IFC como B-rep de faces.
- **Ambientes**: clique dentro de paredes que delimitam ambientes para criar o ambiente; contorno, área e perímetro acompanham as paredes. Etiquetas na planta, tabela de ambientes e exportação como `IfcSpace`.
- **Eixos e anotação**: eixos com bolhas em todas as plantas, nomeados em série ao serem colocados (1, 2, 3… e A, B, C… no sentido transversal), com extremidades que se alinham às bolhas dos outros eixos e o comando *Alinhar extremidades*; cotas alinhadas, lineares, ordenadas (acumuladas a partir de uma origem), angulares, radiais e de diâmetro em qualquer vista 2D (aba *Anotar*); textos de linha simples e multilinha (com quebra automática numa largura, giráveis, editados com duplo clique) com linhas de chamada de um ou dois segmentos terminadas em seta, seta aberta, triângulo, ponto, círculo, quadrado ou traço; marcas de portas e janelas etiquetadas automaticamente.
- **Snapping**: extremidades, pontos médios, sobre a parede, ângulos de 15°, Shift para ortogonal, comprimento digitado + Enter.
- **Edição**: seleção por clique ou janela (contida ou cruzada), mover arrastando, copiar com Ctrl+arrastar, excluir. Desfazer e refazer cobrem todas as edições.
- **Componentes paramétricos**: mobiliário, louças e marcenaria feitos de caixas, cilindros e perfis de retas e arcos desenhados na planta, na frente ou na lateral, extrudados ou girados (revolução), com peças de vazio recortadas das demais; todas as coordenadas e medidas são expressões sobre parâmetros nomeados (do tipo ou da instância). Editor de componentes com verificação imediata e pré-visualização em planta, vista frontal e 3D, biblioteca inicial (mesa, cadeira, cama, sofá, bacia sanitária, lavatório, armário), ferramenta *Componente* (CM), tabelas e legendas, arquivos `.ccomp` para compartilhar e exportação/importação IFC como `IfcFurniture`, `IfcSanitaryTerminal`, `IfcLightFixture` ou `IfcBuildingElementProxy`.
- **Materiais**: navegador com identidade, gráficos (sombreamento, transparência, padrões de superfície e de corte), aparência e propriedades físicas; os padrões de corte aparecem em plantas e cortes, a transparência no 3D.
- **Unidades do projeto**: milímetros, centímetros ou metros, ou unidades imperiais — pés ou polegadas decimais, pés e polegadas fracionárias (`12'-6 1/2"`) ou polegadas fracionárias, até 1/64" — (e formatos de área, volume, ângulo, inclinação e moeda, inclusive pés quadrados e cúbicos, jardas cúbicas e galões americanos) com casas decimais, símbolos e agrupamento de dígitos.
- **Ferramentas de modificação**: mover, copiar, girar, espelhar, matriz, aparar/estender, dividir, deslocar, alinhar, controle de união de paredes e fixação; recortar, copiar e colar, e corresponder propriedades de tipo.
- **Navegação e auxílios de desenho**: cubo de navegação 3D, cotas e ângulos temporários editáveis (digite o comprimento, Tab para o ângulo), linhas de alinhamento, Espaço para girar ou inverter.
- **Instalações (MEP)**: tubulações e eletrodutos (trechos nivelados, inclinados ou verticais, encadeados pelas ferramentas) formando redes com conexões derivadas (joelhos, tês, cruzetas, luvas, reduções) e ligação automática de aparelhos e pontos, dimensionamento de esgoto (UHC), água (vazão e velocidade) e circuitos (disjuntor, condutor, queda de tensão), diagramas de quadro (unifilar e quadro de cargas, nas pranchas), sistemas hidrossanitários e circuitos elétricos com códigos, cores, quadros e tensões, aparelhos sanitários com UHC e pontos elétricos (tomadas, interruptores, pontos de luz, quadros) nas alturas usuais, plantas hidrossanitária e elétrica com etiquetas de diâmetro, declividade e circuitos e a simbologia usual, tabelas de tubulações e eletrodutos e quadro de cargas dos circuitos; aba *Instalações*; exportados como `IfcPipeSegment`, `IfcCableCarrierSegment`, `IfcOutlet`, `IfcSwitchingDevice`, `IfcElectricDistributionBoard` e `IfcDistributionSystem`/`IfcDistributionCircuit`.
- **Propriedades**: parâmetros de instância e da vista em seções recolhíveis; paredes com restrição de base e de topo (altura desconectada ou até um nível), linha de localização, delimitação de ambientes e estrutural.
- **Tipos**: diálogo de propriedades do tipo com duplicar e renomear, editor de camadas com função (núcleo, acabamentos), função da parede e dados de identidade (marca de tipo, fabricante, modelo, classificação, resistência ao fogo, custo…).
- **Vista 3D** (wgpu, MSAA): orbitar, pan, zoom, ajustar ao modelo e seleção por clique.
- **Elevações e cortes**: remoção de linhas ocultas, hachura de corte e níveis anotados.
- **Tabelas** de todas as categorias do modelo e de ambientes: escolha e ordem dos campos (da instância, do tipo e calculados, como comprimento, área e volume), ordenação, filtro, totais e exportação para CSV.
- **Folhas** A0–A4: carimbo com dados do projeto e viewports arrastáveis em escala; exportação vetorial para PDF e SVG (*Arquivo → Exportar PDF/SVG…*), idêntica à tela.
- **Fases de construção** (reformas): elementos são construídos e demolidos em fases ordenadas; cada vista mostra uma fase por um filtro de fase (existente, demolido, novo e temporário pela categoria, substituído ou oculto). *Gerenciar → Fases* edita fases, filtros e seus gráficos; *Modificar → Demolir* (DE) demole elementos na fase da vista.
- **Gerenciar**: informações do projeto, níveis (cada nível novo cria suas plantas) e materiais.
- **Arquivo `.ccad`**: RON versionado e legível; o salvamento é atômico.
- **Exportação IFC4** (*Arquivo → Exportar IFC…*): pavimentos, paredes com aberturas, portas, janelas, pisos, forros, telhados, estrutura e componentes como sólidos extrudados (objetos importados como conjuntos de faces na classe original), com tipos, camadas de materiais, cores e quantitativos. Gerada pelo escritor STEP do próprio ConstructCAD, sem bibliotecas IFC de terceiros.
- **Importação do SketchUp** (*Inserir → Importar SketchUp…*, do SketchUp 3 ao formato atual): componentes e grupos chegam como elementos de componente com a geometria em malhas fixas, uma peça por material, e cópias de um componente compartilham um tipo; *Carregar componente* também aceita um `.skp` como um componente para posicionar. Lido pelo crate `cc_skp` do próprio ConstructCAD.
- **Importação IFC** (*Inserir → Importar IFC…*, IFC2X3 e IFC4) sem ignorar classes: pavimentos viram níveis e eixos viram eixos; paredes, portas, janelas, lajes, forros, telhados, ambientes, pilares e vigas viram elementos nativos com tipos (existentes ou novos), materiais e cores sempre que um elemento nativo representa a geometria exatamente. Todo outro objeto físico — escadas, guarda-corpos, perfis, chapas, fachadas-cortina, tubos e conexões de instalações, terreno, classes de outros esquemas — chega como componente genérico que mantém sua classe IFC, tipo predefinido, conjuntos de propriedades e de quantidades (os seus e os do tipo, convertidos para unidades SI) e geometria exata, e é exportado de volta na mesma classe; as aberturas que o atravessam são recortadas da sua geometria. A geometria é lida pelo motor do próprio ConstructCAD: extrusões (com vazios), revoluções, varreduras de disco e de perfil, primitivas e booleanas CSG (união, diferença, interseção), recortes por semiespaço, B-reps facetados e avançados, conjuntos de faces, itens mapeados, todos os perfis paramétricos e curvas com arcos, cônicas e B-splines.
- **Desenhos DWG e DXF de fundo** (*Inserir → Importar CAD…*, ou solte o arquivo na janela): um desenho colocado numa planta, elevação, corte ou legenda como linhas cinzas de fundo para traçar por cima, com suas camadas, blocos, arcos e textos; snap nas extremidades, pontos médios e centros; unidades, escala, rotação, cores e visibilidade das camadas nas propriedades. O DXF é lido pelo leitor do próprio ConstructCAD; o DWG pela biblioteca de código aberto acadrust.

## Biblioteca de componentes

O [ConstructCAD Components](https://github.com/francisreboucas/constructcad-components) é uma biblioteca gratuita de componentes `.ccomp` prontos: mobiliário, marcenaria, louças e metais, dispositivos elétricos e iluminação, cada um com vários tamanhos como tipos. As louças trazem as unidades Hunter de contribuição e os pesos relativos, e os dispositivos elétricos o tipo e a potência, para o dimensionamento de tubulações e circuitos. Carregue um arquivo com `component_load` (ou `ccad -p casa.ccad --save run component_load path=…`) e insira com a ferramenta *Componente* (CM). Cada arquivo é gerado e verificado pelo próprio `ccad`; veja o [catálogo](https://github.com/francisreboucas/constructcad-components/blob/main/CATALOG.md) e como [contribuir com um componente](https://github.com/francisreboucas/constructcad-components/blob/main/CONTRIBUTING.md).

## Vindo do Revit?

O ConstructCAD não lê arquivos `.rvt`. Para trazer um modelo do Revit, exporte-o para IFC no próprio Revit (*File → Export → IFC*, IFC4 ou IFC2x3) e abra o arquivo em *Inserir → Importar IFC…*. Pavimentos, paredes, portas, janelas, lajes, forros, telhados, ambientes, pilares e vigas chegam como elementos nativos e editáveis, com seus tipos e materiais; todo o resto chega como objetos genéricos com sua classe IFC e geometria, e a mensagem da importação informa quantos.

Quem não tem o Revit pode usar ferramentas independentes de terceiros que convertem `.rvt` para IFC; verifique por conta própria a cobertura e a licença delas. O ConstructCAD não as inclui nem as endossa.

## Atalhos

| Tecla | Ação | Tecla | Ação |
|---|---|---|---|
| WA | Parede | SB | Piso |
| DR | Porta | FO | Forro |
| WN | Janela | TE | Telhado |
| CT | Corte | MD | Selecionar |
| ZE | Ajustar zoom | Esc | Cancelar / sair da ferramenta |
| Ctrl+Z / Ctrl+Y | Desfazer / Refazer | Delete | Excluir |
| Ctrl+S / Ctrl+Shift+S | Salvar / Salvar como | Espaço | Girar / inverter (seleção ou ao posicionar) |
| Ctrl+N / Ctrl+O | Novo / Abrir | Ctrl+W | Fechar projeto |
| RM | Ambiente | GR | Eixo |
| DI | Cota alinhada | CL | Pilar |
| BM | Viga | ST | Escada |
| RA | Guarda-corpo | CM | Componente |
| MV / CO | Mover / Copiar | RO | Girar |
| MM / DM | Espelhar (escolher / desenhar eixo) | AR | Matriz |
| TR / TX | Aparar/estender para canto / até referência | SL | Dividir |
| OF | Deslocamento paralelo | AL | Alinhar |
| PN / UP | Fixar / Liberar | MA | Corresponder propriedades de tipo |
| Ctrl+X / Ctrl+C | Recortar / Copiar | Ctrl+V | Colar |
| TT | Texto | Ctrl+Shift+P | Paleta de comandos |
| Ctrl+F1 | Recolher / expandir o ribbon | SA | Selecionar todas as instâncias na vista |
| GP / UG | Agrupar / Desagrupar | ME | Medir entre duas referências |
| DE | Demolir (na fase da vista) | | |

Navegação 2D: roda do mouse dá zoom e o botão do meio faz pan. Navegação 3D: arrastar com o botão esquerdo orbita, botão direito ou do meio faz pan e a roda dá zoom.

## Automação (linha de comando e MCP)

Tudo o que o ConstructCAD faz deve poder ser acionado sem a interface. O motor (`cc_engine`) expõe as operações como comandos nomeados com parâmetros JSON e um esquema JSON cada; a ferramenta `ccad` os executa pela linha de comando, por scripts ou, para agentes de IA, como servidor [MCP](https://modelcontextprotocol.io/). O mesmo catálogo atende os três. O [guia de automação](docs/automacao.md) explica tudo em detalhe e a [referência de comandos](docs/commands.md) lista cada comando com seus parâmetros.

```sh
ccad commands                                   # lista os comandos
ccad schema wall_create                         # parâmetros de um comando
ccad -p casa.ccad --save run wall_create start=[0,0] end=[5,0] height=2.8
ccad -p casa.ccad run elements_list category=door
ccad -p modelo.ifc --save-as modelo.ccad run project_info   # importa um IFC
ccad -p casa.ccad script obra.jsonl             # um {"command": ..., "args": {...}} por linha
```

Comprimentos em metros, ângulos em graus, pontos `[x, y]`; níveis, tipos, vistas e materiais podem ser indicados por id ou por nome. `element_get`, `type_get` e `view_get` mostram os `params` que `element_set`, `type_set` e `view_set` alteram. Cada edição é um passo de desfazer, e uma edição que deixaria o modelo inválido é recusada.

Para usar num cliente MCP (Claude Desktop, Claude Code, IDEs), registre `ccad mcp`:

```json
{ "mcpServers": { "constructcad": { "command": "ccad", "args": ["mcp"] } } }
```

ou, no Claude Code, `claude mcp add constructcad -- ccad mcp`. Cada sessão do servidor tem um projeto aberto (`project_open`, `project_new`, `ifc_import`, `project_save`).

O aplicativo roda sobre os mesmos comandos: ferramentas de desenho, painel de propriedades e diálogos alteram o modelo pelo motor. *Gerenciar → Macros* grava o que você faz como uma macro (um script de comandos, salvo como `.ccmacro`) e executa macros; `ccad script` roda os mesmos arquivos.

## Arquitetura

```
crates/
  cc_core    modelo BIM (elementos, tipos, níveis, vistas), transações undo/redo, persistência .ccad
  cc_geom    geometria derivada: paredes com junções e aberturas, lajes, telhados, cortes, picking
  cc_views   geração de desenhos 2D (planta, forro, elevação, corte, axonometria, legenda), tabelas, montagem das folhas e exportação PDF/SVG
  cc_render  renderizador 3D offscreen em wgpu
  cc_ifc     exportação IFC4 e importação IFC2X3/IFC4: escritor e leitor STEP, mapeamento do modelo
  cc_engine  cada operação como comando nomeado com esquema JSON, numa sessão sem GUI
  cc_cli     `ccad`: linha de comando, scripts e servidor MCP sobre o motor
  cc_app     aplicativo eframe/egui: ribbon, navegador, propriedades, ferramentas, folhas
assets/i18n  traduções (pt-BR, en); textos de interface nunca ficam fixos no código
```

Toda alteração passa por `Document::edit`, que registra uma `Transaction` reversível. As edições contínuas (arrastar valores) são agrupadas em um único passo de desfazer.

## Desenvolvimento

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
CC_UI_SNAPSHOTS=1 cargo test -p cc_app snapshots    # capturas da interface (veja docs/interface.pt-BR.md)
```

Os arquivos IFC exportados são validados na CI com o [IfcOpenShell](https://ifcopenshell.org/), uma referência independente e de código aberto usada apenas nos testes:

```sh
pip install -r scripts/requirements-ifc.txt
cargo run -p cc_ifc --example export_ifc -- examples/casa_simples.ccad casa_simples.ifc
python scripts/validate_ifc.py casa_simples.ifc
```

## Próximos passos

- Componentes em série, aninhados e hospedados (em paredes e forros).
- Importação nativa (editável) de escadas e guarda-corpos.
- Executar uma macro como um único passo de desfazer; parâmetros em macros.
- Texturas de materiais.

## Independência

O ConstructCAD é um projeto independente. O comportamento do programa foi projetado com base em documentação pública, convenções gerais de CAD e retorno dos usuários. Os testes de referência usam valores analíticos e ferramentas de código aberto. O ConstructCAD não tem afiliação com a Autodesk, nem é patrocinado ou endossado por ela. Autodesk e Revit são marcas registradas da Autodesk, Inc.

> ConstructCAD is independent. Behaviour is designed from public documentation, general CAD conventions and user feedback. Reference tests use analytic values and open-source tools. ConstructCAD is not affiliated with, sponsored by or endorsed by Autodesk. Autodesk and Revit are trademarks of Autodesk, Inc.

## Licença

Copyright © Francis Rebouças <francisreboucas@gmail.com>.

O ConstructCAD é distribuído sob a [GNU General Public License v3.0 ou posterior](LICENSE) (GPL-3.0-or-later).

O ícone do aplicativo é de [Magnific – Flaticon](https://www.flaticon.com/) (veja `assets/icons/FLATICON.txt`). A fonte [Inter](https://rsms.me/inter/) incluída é © The Inter Project Authors e distribuída sob a [SIL Open Font License 1.1](assets/fonts/OFL-Inter.txt).
