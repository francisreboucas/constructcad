# A interface: ribbon, comandos e paleta

*English version: [interface.md](interface.md).*

A janela do ConstructCAD segue a disposição que usuários de CAD e BIM de desktop esperam. A **barra de acesso rápido** fica no canto superior esquerdo. Abaixo dela está o **ribbon**, uma fileira de abas com grupos de botões. Em seguida vem a **barra de opções** da ferramenta ativa. O **navegador de projeto** e as **propriedades** ficam encaixados à esquerda, as **vistas** ocupam as abas no centro e a **barra de status** fica embaixo. Com muitas vistas abertas, **Fechar vistas inativas** (na barra de acesso rápido e em *Vista → Janelas*) fecha todas as abas menos a ativa, e o clique direito numa aba oferece *Fechar as outras vistas*.

## Comandos e suas dicas

Todo botão do ribbon e da barra de acesso rápido executa um *comando da interface*. O mesmo vale para cada atalho de duas letras e cada item da paleta de comandos. Cada comando define, num só lugar:

- seu **nome** e **ícone**;
- seu **atalho**: uma combinação com Ctrl ou duas letras digitadas com o foco numa vista;
- uma **descrição** do que faz;
- o **comando do motor** que faz a alteração. Scripts, a linha de comando `ccad` e o servidor MCP chamam esse mesmo comando (veja o [guia de automação](automacao.md));
- se ele **pode rodar agora** e, se não puder, por quê.

Passe o mouse sobre um botão para ver a dica:

```
Mover  [MV]
Move os elementos selecionados de um ponto base até um destino.
Indisponível: Selecione os elementos antes de usar a ferramenta
Comando do motor: elements_move
```

Botões que não podem rodar ficam acinzentados, e a dica explica o motivo. Motivos comuns são "nada para desfazer", "selecione os elementos primeiro", "abra uma folha para exportá-la", "selecione um texto" e "copie elementos primeiro". Um atalho digitado quando o comando não pode rodar mostra o mesmo motivo na barra de status.

## Ícones e selos

Os ícones têm dois tons: os traços na cor do texto e os preenchimentos suaves no tom de destaque do tema. Um pequeno selo no canto inferior direito indica o tipo de ação do comando:

| Selo | Significado | Exemplos |
|---|---|---|
| **+** verde | cria algo novo | nova planta de piso ou de forro, vista 3D, legenda, tabela, folha, novo componente |
| seta azul para baixo | traz dados de um arquivo | Importar IFC, Importar CAD, Carregar componente |
| seta azul para cima | grava dados num arquivo | Exportar PDF, Exportar SVG, Exportar imagem, Exportar DXF |
| **−** vermelho | remove algo | Excluir |

A ferramenta selecionada aparece realçada na cor de destaque. Comandos indisponíveis ficam acinzentados, inclusive o selo.

## Recolher o ribbon

Para dar mais espaço às vistas, recolha o ribbon até os títulos das abas. Qualquer uma destas ações recolhe o ribbon ou o expande de novo:

- **Ctrl+F1**;
- um **clique duplo** em qualquer aba;
- a **seta** no fim da fileira de abas, à direita.

Com o ribbon recolhido, um clique numa aba mostra o painel dela por cima das vistas. O painel fecha quando um comando é executado, quando você aperta Esc ou clica em outro lugar, ou quando clica de novo na mesma aba. Se o ribbon começa recolhido é definido em *Preferências → Interface*.

## Janelas estreitas

Quando a janela é mais estreita que os grupos de uma aba, os grupos rolam de lado em vez de serem cortados. Aparecem setas nas duas pontas da fileira: clique nelas para rolar, ou use a roda do mouse. Cada seta fica acinzentada ao chegar à sua ponta.

## Paleta de comandos

**Ctrl+Shift+P** abre a paleta de comandos, assim como a lupa no fim da fileira de abas, à direita. Digite parte do que procura. A busca ignora maiúsculas e acentos ("niveis" encontra "Níveis"), e todas as palavras digitadas precisam aparecer. A paleta encontra:

- **comandos da interface**, pelo nome, pela descrição ou pelo atalho (digitar `WA` encontra *Parede*);
- **plantas a criar** para cada nível ("Planta de piso: Térreo") e **tabelas** para cada categoria;
- **vistas a abrir** ("Abrir vista: …");
- **comandos do motor** (`level_create`, `elements_list`, `type_set`…), em fonte monoespaçada.

Use ↑ e ↓ para escolher e Enter para executar, ou clique num item. Esc fecha a paleta. Um comando indisponível continua na lista, acinzentado, com o motivo no lugar da descrição.

### Executar comandos do motor

Escolher um comando do motor abre um formulário com sua descrição e seus argumentos em JSON, já com os campos obrigatórios:

```json
{
  "elevation": 9.0,
  "name": "Cobertura"
}
```

**Executar** (ou Ctrl+Enter) roda o comando no projeto aberto e mostra o resultado ou a mensagem de erro. Uma alteração é um passo de desfazer e é gravada enquanto uma macro está gravando, como qualquer outra. **Voltar** (ou Esc) retorna à busca. Os argumentos são os mesmos dos scripts e da [referência de comandos](commands.md). Comprimentos estão em metros, ângulos em graus, pontos são `[x, y]`, e níveis, tipos, vistas e materiais podem ser indicados pelo id ou pelo nome.

Os comandos de arquivo do motor (`project_open`, `project_save`, `ifc_import`, `ifc_export`, `sheet_export`, `view_export`…) não aparecem aqui. Os comandos da interface correspondentes, nas abas *Arquivo* e *Inserir* e na paleta, cuidam dos arquivos pelos diálogos de sempre.

## Teclado

| Tecla | Ação |
|---|---|
| Ctrl+Shift+P | Paleta de comandos |
| Ctrl+F1 | Recolher / expandir o ribbon |
| Ctrl+, | Preferências |
| Ctrl+N / Ctrl+O / Ctrl+S / Ctrl+Shift+S / Ctrl+W | Novo / Abrir / Salvar / Salvar como / Fechar projeto |
| Ctrl+Z / Ctrl+Y | Desfazer / Refazer |
| Ctrl+X / Ctrl+C / Ctrl+V | Recortar / Copiar / Colar |
| Delete | Excluir a seleção |
| Esc | Cancelar a operação, sair da ferramenta, limpar a seleção |
| Backspace | Ao desenhar uma sequência de paredes, vigas, tubos ou eletrodutos: desfaz o último segmento e continua do início dele (Ctrl+Z também; Ctrl+Y o refaz); o último ponto de um guarda-corpo |
| Espaço | Girar ou inverter a seleção ou o elemento sendo posicionado |

As teclas de duas letras funcionam com o foco numa vista e nenhum campo de texto em edição. O README lista todas elas, e cada dica mostra também o seu atalho.

## Selecionar em vistas 2D

Um clique escolhe o elemento sob o cursor (Ctrl soma ou retira). Um arrasto que começa no papel vazio, ou dentro de uma laje, de um hachurado ou de outro preenchimento, desenha uma caixa de seleção: arrastada para a direita é uma janela e pega os elementos inteiramente dentro dela (linhas, preenchimentos e textos); para a esquerda é um cruzamento, tracejado e verde, e pega também os que ela corta ou em que está: uma parede cortada entre as pontas, uma laje em volta da caixa, um texto ancorado nela. Áreas de ambiente e desenhos importados não entram por cruzamento (uma janela em volta deles os pega). Ctrl mantém a seleção atual e soma a ela. Um arrasto que começa na linha de um elemento, ou num elemento já selecionado, o move (Ctrl+arrastar copia).

Os identificadores de portas, janelas, vigas, pilares e sapatas podem ser arrastados para outro lugar com a ferramenta Selecionar (o cursor vira uma mão sobre eles). O identificador mantém o texto e ganha uma linha de chamada até o elemento; cada planta guarda as posições dos seus identificadores (uma planta duplicada começa com as da original). *Repor identificador*, nas propriedades do elemento, o devolve ao lugar padrão.

## Editar vários elementos

Com vários elementos de uma categoria selecionados, a paleta de propriedades mostra *Tipo* e *Nível* e os parâmetros que eles têm em comum (deslocamentos da base e do topo, a altura quando o topo está livre, peitoril, beiral, rotação, *Estrutural*, *Delimita ambientes*, inversões). Um valor que difere entre eles aparece como *‹vários›* (uma caixa indeterminada nos de ligar e desligar); escolher ou digitar um valor o aplica a todos num só passo de desfazer (o comando `elements_set`; tipos por `elements_set_type`). Uma seleção de categorias diferentes mostra quantos há de cada uma e *Excluir*.

## Selecionar em vistas 3D

Com a ferramenta Selecionar, um clique escolhe o elemento sob o cursor (Ctrl soma ou retira). Shift+arrastar desenha uma caixa de seleção, como nas plantas: arrastada para a direita é uma janela e pega os elementos inteiramente dentro dela; para a esquerda é um cruzamento e pega também os que ela toca. Elementos atrás de outros contam; os ocultos, não. Ctrl mantém a seleção atual e soma a ela. Com *Arrastar na vista 3D seleciona* ligado em *Preferências → Interface*, arrastar com o botão esquerdo desenha a caixa e Shift+arrastar orbita.

## Modificar em vistas 3D

**Mover**, **Copiar**, **Matriz**, **Girar**, **Espelhar** (escolher ou desenhar o eixo), **Alinhar** e **Unir geometria**/**Desunir geometria** também funcionam em vistas 3D; arrastar com o botão esquerdo continua orbitando (ou Shift+arrastar, veja acima), com o do meio ou o direito desloca a vista.

- Os pontos são escolhidos num plano de trabalho horizontal no nível dos elementos alterados; depois do primeiro ponto, os seguintes ficam na altura dele. Um quadrado indica a atração a um ponto do modelo perto do cursor: um canto de elemento, o centro de um pilar ou componente na base ou no topo, a ponta de uma parede, o cruzamento de eixos. Uma cruz indica um ponto no plano. Shift mantém o eixo principal; digitar a distância e Enter funciona como nas plantas. A prévia mostra o contorno dos elementos onde eles vão ficar.
- Toda alteração é uma transformação em planta: os elementos continuam no seu nível. Mover um pilar do pavimento 1 de um canto até o canto correspondente do pilar de baixo o coloca sobre esse pilar.
- **Alinhar** (AL, também nas plantas): clique na face ou linha de referência, depois na face ou linha do elemento a mover. Enquanto você aponta, a face ou linha que o clique pegaria aparece mais grossa; escolhida a referência, ela fica marcada por uma linha vermelha tracejada que cruza a vista, até o segundo clique ou Esc. O elemento se move em planta, então os dois podem estar em níveis diferentes (um pilar de cima sobre a face do pilar de baixo). Todo elemento se alinha pela face ou lado mais próximo do clique (uma parede por qualquer das faces, um pilar por um dos lados); eixos, e elementos escolhidos pela linha de comando sem um ponto, pela linha de locação. Quando os dois não são paralelos, o elemento primeiro gira em torno do ponto clicado até ficarem.
- **Aparar/estender para canto**, **Aparar/estender até referência**, **Dividir elemento**, **Deslocamento paralelo** e **Permitir/impedir união** escolhem pontas e lados nas linhas da planta e ficam indisponíveis em vistas 3D, com esse motivo na dica.

## Gráficos das vistas

*Vista → Gráficos* reúne **Visibilidade/gráficos** (VG), **Filtros** e **Modelos de vista** (também em *Gerenciar*).

- **Visibilidade/gráficos** edita a vista ativa (ou, a partir do gerenciador de modelos, um modelo). As abas *Categorias de modelo* e *Categorias de anotação* listam cada categoria com uma caixa de visibilidade e células para linhas em projeção, preenchimento de superfície, linhas de corte, preenchimento de corte, meio-tom e transparência; a célula mostra a sobreposição (— quando não há) e abre um editor de cor, espessura (mm de papel) e padrão. A aba *Filtros* lista os filtros aplicados à vista em ordem de prioridade (↑ ↓), com caixas de ativo e visível e as mesmas células; *Adicionar filtro…* aplica outro e *Editar filtros…* abre o editor de filtros. Quando o modelo da vista controla categorias ou filtros, a aba avisa e fica só para leitura. OK mantém as mudanças; Cancelar as desfaz.
- **Filtros** edita os filtros do projeto: nome, categorias (nenhuma: todas) e regras — parâmetro, operador e valor — combinadas com E ou OU, com o número de elementos que correspondem.
- **Modelos de vista** lista os modelos; *Criar a partir da vista* cria um da vista ativa. Cada propriedade tem a caixa *Incluir*; *Aplicar propriedades* copia o modelo para a vista ativa uma vez, *Atribuir à vista* faz a vista segui-lo. As propriedades da vista mostram seu modelo (*Nenhum* a libera) e um botão *Visibilidade/gráficos*; as propriedades controladas pelo modelo ficam só para leitura.
- **Nível de detalhe** (*Baixo*, *Médio*, *Alto*), na seção *Gráficos* das propriedades de plantas, fachadas, cortes e vistas 3D, e propriedade dos modelos de vista: no *Baixo*, paredes, pisos e telhados em camadas são cortados como uma região lisa (sem linhas de camada nem hachuras de material); no *Médio* (o padrão) e no *Alto* mostram suas camadas. Cada peça de componente tem as caixas *Aparece em* para os três níveis (todas marcadas por padrão), então um componente pode desenhar um bloco simples nas vistas de nível baixo e seus detalhes só nas de nível alto; portas e janelas também.
- Com elementos selecionados, **Sobrepor gráficos** muda seus gráficos só na vista ativa e **Ocultar na vista** (EH) os oculta nela (reexibidos em Visibilidade/gráficos).
- **Ocultar/isolar** (temporário): HH oculta a seleção, HI a isola, HC e IC ocultam ou isolam suas categorias, HR redefine. Enquanto ativo, a vista ganha uma borda colorida e um aviso com *Redefinir* e *Tornar permanente*. A ocultação temporária não é salva, não entra no desfazer e não é impressa.

## Esboço de contornos

Pisos, forros, telhados, radiers e massas são esboçados em planta. A ferramenta abre um esboço: o ribbon mostra *Criar contorno* com as ferramentas de desenho até *Concluir* ou *Cancelar*.

- **Desenhar**: linhas (encadeadas até fechar o laço; *Encadear* na barra de opções), retângulos, polígonos inscritos e circunscritos (*Lados* na barra de opções), círculos, elipses, arcos por três pontos, pelo centro e arcos tangentes que continuam a ponta livre de uma linha ou arco. Os pontos se ajustam ao modelo e às pontas, meios e centros do próprio esboço. *Das paredes* adiciona linhas ao longo das paredes do nível.
- **Selecionar**: um clique escolhe uma curva (Ctrl adiciona ou remove), um clique duplo a cadeia de curvas ligadas a ela, uma janela as curvas dentro dela (arrastada para a esquerda, as que ela toca); suas alças a esticam (uma ponta arrasta as pontas ligadas a ela, e os laços continuam fechados; a alça do meio de um arco muda sua curvatura, as do círculo e da elipse os raios). A paleta de propriedades mostra início, fim, centro, raio, semieixos e rotação da curva escolhida, para digitar. Delete remove as curvas escolhidas; Ctrl+Z e Ctrl+Y desfazem e refazem as mudanças do próprio esboço.
- **Modificar**: *Mover*, *Copiar* (*Múltiplo* coloca cópias a partir do mesmo ponto base), *Girar* (centro, direção de referência e final, ou um ângulo digitado e *Aplicar*) e *Espelhar* (por uma linha do esboço, ou por um eixo desenhado com dois pontos; *Copiar* mantém os originais) agem sobre as curvas escolhidas. *Deslocamento paralelo* copia a curva sob o cursor à *Distância* no lado clicado; com *Encadear* copia a cadeia inteira de curvas ligadas, e as vizinhas se encontram onde as cópias se cruzam (elipses não têm deslocamento). *Aparar/estender para canto* faz duas curvas se encontrarem, mantendo as partes clicadas; *Aparar/estender até referência* toma uma curva de referência e apara ou estende até ela cada curva clicada; linhas seguem pela sua reta, arcos pelo seu círculo. *Dividir elemento* corta uma curva em duas onde ela é clicada (um círculo ou uma elipse inteira em metades, que depois podem ser aparadas). Os atalhos de duas letras dessas ferramentas também funcionam no esboço.
- **Concluir** cria o elemento. Laços dentro do contorno viram aberturas de pisos e telhados. Um esboço aberto, que se ramifica ou se cruza continua aberto, com o lugar marcado e o motivo na barra de status.
- **Editar contorno** (*Modificar → Esboço*) abre o esboço do piso, forro, telhado ou massa selecionado: suas curvas como foram desenhadas, ou linhas ao longo do contorno. Mover, girar e espelhar o elemento mantém as curvas; editar diretamente os pontos as descarta.

## Cobertura e estrutura de telhados

Um tipo de telhado define a **telha** e a **estrutura**, na seção *Cobertura e estrutura* do diálogo de tipo.

- **Telha**: cerâmica (colonial, francesa, romana, portuguesa), telha de concreto, fibrocimento ondulada, metálica trapezoidal ou telha sanduíche. Cada uma traz valores de referência: a inclinação mínima, o peso por m², as telhas por m², o espaçamento das ripas (galga) e o maior vão entre terças. São valores típicos de dados públicos de fabricantes, para o estudo; os do fabricante prevalecem.
- **Estrutura**: *Madeira* põe as ripas logo abaixo do telhado na galga da telha, os caibros descendo a água abaixo delas (no espaçamento dos caibros) e as terças ao longo do beiral abaixo dos caibros, distribuídas por igual do beiral à cumeeira, sem passar do espaçamento das terças (ou do vão da telha, se menor). *Tesouras de madeira* acrescenta tesouras abaixo das terças, atravessando a cumeeira e sem passar do espaçamento das tesouras: pernas ao longo das águas, linha (tirante) nivelada, pendural sob a cumeeira e escoras até o meio de cada perna. *Terças metálicas* põe só as terças, logo abaixo das telhas. Seções, espaçamentos e material ficam no tipo. Escolher a telha escolhe a estrutura em que ela costuma assentar, se nenhuma foi escolhida.
- A estrutura segue as águas do telhado, qualquer que seja a forma, e aparece nas vistas 3D, elevações e cortes com o seu material. Ela não entra no volume do telhado. A exportação IFC grava cada peça como um `IfcMember` dentro do telhado, e importar esse arquivo traz a telha e a estrutura de volta no tipo do telhado.
- As propriedades do telhado mostram a telha, a inclinação contra a mínima da telha (em vermelho quando abaixo), o número de telhas, o peso e os comprimentos e o volume da estrutura. As tabelas de telhados podem listar *Inclinação*, *Telhas*, *Peso*, *Estrutura (comprimento)* e *Estrutura (volume)*. O comando `roof_check` devolve o mesmo, com as regras que o telhado não atende.

## Massas conceituais

*Arquitetura → Estudo de massa → Massa* (`MS`) desenha um contorno na planta, como uma laje, e o extruda até o nível de cima. As massas servem para estudos de volumetria antes de existirem paredes e lajes: corpos transparentes nas vistas 3D, elevações e cortes, cortados nas plantas que atravessam.

- **Propriedades**: o deslocamento da base e a altura, e se a massa é uma extrusão ou uma transição (um contorno de topo diferente da base, feito com `mass_create` e `top` ou `top_inset`).
- **Pisos de massa**: uma caixa por nível que a massa atravessa, com a área do piso naquele nível. As plantas desenham o piso do seu nível com a área; as propriedades e a tabela de *Massas* dão a área bruta de pisos (a soma dos pisos), o número de pisos e o volume.
- **Empurrar/Puxar** (*Estudo de massa → Empurrar/Puxar*, `PP`, numa vista 3D): clique numa face da massa, mova o cursor ao longo da seta (ou digite a distância) e clique ou aperte Enter. O topo muda a altura, a base desce ou sobe, e um lado de uma extrusão se desloca para fora ou para dentro, os lados vizinhos mantendo a direção (os lados de uma transição ficam; empurre o topo ou a base). Uma massa com piso em todos os níveis que atravessa ganha os pisos dos níveis que passa a alcançar. Esc solta a face.
- **Da massa ao edifício**: com massas selecionadas, *Estudo de massa → Paredes por face* põe uma parede em cada lado de uma extrusão (face externa na face da massa, com a altura dela, unidas nos cantos), *Lajes dos pisos de massa* uma laje em cada piso de massa (o contorno da massa naquele nível, com o topo no nível) e *Telhado por face* um telhado plano no topo. Paredes, lajes e telhado são elementos comuns: mude tipo, altura e forma como de costume. Os lados inclinados de uma transição não recebem paredes.
- Mover, girar, espelhar, copiar e colar funcionam como nos outros elementos. Excluir um nível remove os pisos de massa dele. A exportação IFC deixa as massas de fora.

## Guarda-corpos e seus hospedeiros

*Arquitetura → Circulação → Guarda-corpo* (`RA`) desenha um guarda-corpo pelos pontos clicados, no nível mais o deslocamento da base. Um guarda-corpo também pode acompanhar uma escada ou rampa (seu hospedeiro): ele corre pelo lado do hospedeiro na inclinação dos bocéis ou da rampa, contorna os patamares e se move, muda e é excluído com ele.

- **Numa escada ou rampa**: marque *Numa escada ou rampa* na barra de opções da ferramenta e clique numa escada ou rampa em planta. O guarda-corpo vai no lado mais próximo do clique, ou nos dois com *Dos dois lados*. As propriedades da escada ou rampa também têm *Adicionar guarda-corpos*, para os dois lados de uma vez.
- **Lado e hospedeiro**: as propriedades de um guarda-corpo hospedado mostram o hospedeiro e o *Lado* (subindo), que pode ser trocado. *Soltar do hospedeiro* o torna um guarda-corpo comum, que mantém o caminho que tem em planta.
- Pela linha de comando ou MCP: `stair_railings_add` (com `near`, para o lado mais próximo de um ponto) e `railing_host_set` (põe um guarda-corpo existente numa escada ou rampa, no lado por onde seu caminho passa se `side` não for dado; troca o lado ou o recuo; ou o solta com `detach`).

## Perfis varridos

*Arquitetura → Modelo → Perfil varrido* (`SW`) leva um perfil ao longo de um caminho desenhado em planta: rodapés, molduras de teto, testeiras, calhas, rufos. Um projeto sem tipo de perfil varrido ganha um na primeira vez (um rodapé de 1,5 × 10 cm).

- **Desenho**: clique os pontos do caminho; Enter, clique duplo ou *Concluir* encerra, e Backspace remove o último ponto. A barra de opções define a altura do caminho acima do nível, *Caminho fechado* (ele volta ao primeiro ponto, em volta de um ambiente, sem pontas) e *Inverter lado*.
- **De que lado**: o perfil é desenhado num plano próprio, perpendicular ao caminho: x na transversal, à esquerda de quem anda pelo caminho, e y para cima a partir da altura do caminho. *Inverter lado* o põe à direita; uma cópia espelhada o mantém do mesmo lado da parede que acompanha. Os cantos ficam em meia-esquadria.
- **Perfis**: o tipo guarda o perfil e o material. `sweep_type_create` cria um tipo a partir de pontos `[x, y]` ou de `curves` (linhas, arcos, círculos, retângulos… formando um contorno fechado, então uma meia-cana ou uma calha meia-cana se desenham com arcos), e `sweep_profile_set` redesenha o perfil de um tipo; os dois estão na paleta de comandos. Um perfil precisa ser um contorno fechado que não se cruza.
- **Vistas**: plantas e plantas de forro desenham o que o plano de corte corta e o que fica abaixo (ou acima, nas plantas de forro), elevações, cortes e vistas 3D mostram o sólido, legendas mostram a seção do perfil e tabelas listam comprimento e volume. A exportação IFC os grava como revestimentos do tipo `MOLDING`.

## Desenhos CAD importados

- **Inserir → Importar CAD…** coloca um desenho DWG ou DXF na planta, planta de forro, elevação, corte ou legenda ativa (nas outras vistas o botão fica cinza e diz por quê). Também dá para soltar arquivos `.dwg` e `.dxf` na janela. O diálogo de arquivo começa na *Pasta de projetos*. A origem do desenho fica na origem da vista, a menos que isso o deixe a mais de 1 km do modelo (um desenho em coordenadas de levantamento); então ele é centrado no que a vista mostra. A barra de status informa quantos itens e camadas foram lidos, quantas entidades não foram desenhadas (sólidos 3D, imagens, linhas infinitas…) e que unidades foram supostas quando o arquivo não tem nenhuma.
- **Inserir → Importar imagem…** coloca uma imagem (PNG, JPEG, GIF, BMP, WebP) ou um desenho SVG na vista ativa, nas mesmas vistas do *Importar CAD*, centralizada no que a vista mostra; soltar um desses arquivos na janela faz o mesmo. A imagem fica no papel do tamanho que sua resolução dá (96 dpi quando o arquivo não declara): ajuste a *Escala* nas propriedades (100 para uma planta escaneada em 1:100) ou `width` em `image_import`. O SVG mantém caminhos, preenchimentos e imagens embutidas nas próprias cores (os textos não são lidos). É um desenho importado como um DWG: fixado, movido, girado e oculto do mesmo jeito, impresso e exportado com a vista.
- **Arquivo → Exportar DXF…** (também *Inserir → Exportar DXF…*) grava a vista ativa (qualquer vista menos tabela) como desenho DXF para outros programas CAD, como impressa: plantas, fachadas e cortes em tamanho real em milímetros (textos na altura impressa vezes a escala da vista), a folha em milímetros de papel. Cada elemento vai para a camada da sua categoria (`WALL`, `DOOR`…; anotações em `ANNOTATION` e `TEXT`), linhas tracejadas e traço-ponto recebem tipos de linha, preenchimentos opacos viram sólidos e as cores são o índice de cor mais próximo, para que uma tabela de penas por cor dê as espessuras. O arquivo é DXF R12, que todo programa CAD abre; ele volta com *Importar CAD*.
- O desenho é um fundo para traçar por cima: linhas finas cinzas sob o modelo, com seus textos. Ele aparece só na vista em que foi importado (não em outras plantas nem em vistas 3D), sai nas folhas e exportações e fica fora da exportação IFC. *Visibilidade/gráficos* o oculta ou sobrepõe pela categoria *CAD importado*.
- Ele começa **fixado**, para não ser movido sem querer; use *Liberar* para movê-lo ou girá-lo com as ferramentas de sempre. Um clique nas linhas dele só o seleciona onde não há elemento do modelo sob o cursor.
- As propriedades definem as **unidades** (lidas do arquivo, senão supostas), uma **escala** extra, a **rotação**, a **posição** da origem, as **cores** (*Cinza*, *Cores das camadas*, com as cores muito claras escurecidas para o papel branco, ou *Preto*) e o **Meio-tom**. A seção *Camadas* mostra ou oculta cada camada (*Todas as camadas* alterna todas); camadas desligadas ou congeladas no arquivo começam ocultas.
- As ferramentas de desenho fazem snap nas extremidades e vértices, pontos médios, centros de arcos e círculos (um marcador de círculo) e em pontos das linhas dele.

## Fases de construção

- **Gerenciar → Fases** tem três abas. *Fases* lista as fases do projeto da mais antiga para a mais recente, com nome, descrição e quantos elementos são construídos e demolidos em cada uma; *Adicionar* acrescenta uma no fim, *Inserir antes* põe uma antes da selecionada, ↑ ↓ reordenam (uma mudança que poria uma demolição antes da construção é recusada), e excluir uma fase passa seus elementos e vistas para a fase escolhida em *Mesclar elementos e vistas em*. *Filtros de fase* define, para cada filtro, como aparecem os elementos existentes, demolidos, novos e temporários: *Não exibido*, *Por categoria* ou *Substituído*. *Gráficos de fase* define as sobreposições de cada estado (as mesmas células de Visibilidade/gráficos). OK mantém as mudanças; Cancelar as desfaz.
- As propriedades da vista (plantas, elevações, cortes, vistas 3D, tabelas) têm a seção *Fases* com sua *Fase* e *Filtro de fase*; o filtro fica só para leitura quando o modelo da vista o controla. Os elementos mostram *Fase de criação* e *Fase de demolição*. Elementos novos são construídos na fase da vista ativa.
- **Modificar → Demolir** (DE) demole os elementos clicados na fase da planta, elevação ou corte ativo; suas portas e janelas vão junto.

## Ambiente de modelagem de componentes

*Editar componente* (*Arquitetura → Modelo*, ou *Editar componente…* nas propriedades de um tipo de componente) abre um espaço de trabalho próprio para o componente selecionado, ou para o que a ferramenta *Componente* coloca. *Edição rápida…* mantém o editor antigo, numa só janela.

- **Vistas:** as vistas do projeto dão lugar às vistas *Planta*, *Frente*, *Lateral* e *3D* do componente. A rodinha aproxima, arrastar com o botão do meio ou o direito move a vista, e um duplo clique com o botão do meio enquadra tudo.
- **Ribbon:** a aba *Criar* traz *Selecionar*, *Extrusão*, *Revolução*, *Varredura*, *Transição*, *Plano de referência*, *Cota*, *Igualdade (EQ)*, *Conector*, *Salvar componente…*, *Concluir* e *Cancelar edição*.
- **Painel à esquerda:** o tipo cujos valores as vistas mostram, os parâmetros (nome, valor, se os elementos podem sobrepô-lo, × e *+ Parâmetro*) e as propriedades do plano, da cota, da peça ou do conector selecionados.
- **Planos de referência:** três planos de origem estão sempre lá: *CenterX* (centro esquerda/direita), *CenterY* (centro frente/fundo) e *Base*. *Plano de referência* leva dois cliques na vista de planta, frente ou lateral; uma linha mais vertical ou mais horizontal define a orientação do plano. A posição de um plano é uma expressão (`Left + Largura`), e seu nome pode ser usado em qualquer expressão do componente, então uma peça que lê `x = Left` acompanha o plano.
- **Cotas:** *Cota* pega dois planos paralelos e os mede onde você clicou. Nas propriedades, um *Rótulo* (um parâmetro de comprimento, ou um novo digitado em *Novo parâmetro*) faz o parâmetro controlar o segundo plano. Mudar o valor muda o parâmetro, ou move o plano quando a cota não tem rótulo.
- **Peças por esboço:** *Extrusão*, *Revolução*, *Varredura* e *Transição* abrem um esboço, o mesmo dos contornos em planta (veja *Esboço de contornos*): seu ribbon (*Esboço: Extrusão*…) traz as ferramentas de desenho (linhas, retângulos, polígonos, círculos, elipses e arcos), as ferramentas de Modificar, *Concluir* e *Cancelar*; as curvas são escolhidas, esticadas pelas alças e digitadas no painel lateral. Desenhe na vista de planta, frente ou lateral: a primeira curva fixa a vista. Os pontos se ajustam aos pontos do próprio esboço e aos planos de referência, e uma coordenada sobre um plano visto de perfil fica presa a ele, então a peça acompanha o plano; pontos de um círculo ou elipse com centro sobre um plano são escritos a partir desse plano. A barra de opções define *Sólido* ou *Vazio*. *Concluir* cria a extrusão na hora; a revolução pede em seguida o eixo, um plano visto como linha vertical ou qualquer ponto (Esc volta ao contorno). A peça nova fica selecionada: suas propriedades definem o nome, *Vazio*, *Mostrar em planta*, o material e onde ela começa e termina (ou o eixo e o ângulo), cada um uma expressão que o menu ⌖ prende a um plano. *Editar esboço* nas propriedades reabre o contorno (o caminho de uma varredura, a base e depois o topo de uma transição); pontos que não se moveram mantêm as expressões com que foram escritos.
- **Igualdades (EQ):** *Igualdade (EQ)* pega três planos paralelos numa vista: um externo, o que fica no meio e o outro externo. A posição do plano do meio passa a ser `(a + b) / 2`, então ele continua centrado quando os outros se movem; a vista mostra duas cotas marcadas EQ. Mover o plano do meio à mão quebraria a igualdade e é recusado; apague a igualdade antes.
- **Conectores:** onde tubos e eletrodutos se ligam ao componente (a entrada de água e o esgoto de uma peça sanitária, a ligação elétrica de uma tomada). *Conector* coloca um com um clique na vista de planta, frente ou lateral; uma coordenada perto de um plano de referência visto de perfil fica presa a ele, então o conector acompanha o plano, e a profundidade começa em 0 (em planta ele aponta para baixo, nas outras vistas para trás). Todas as vistas, a 3D também, mostram os conectores como anéis na cor do seu sistema, com uma seta no sentido em que as tubulações saem (um ponto quando é na sua direção, um x quando é para longe). As propriedades definem o nome, o tipo de sistema, *X*, *Y* e *Z* (expressões que o menu ⌖ prende a um plano), a direção e o diâmetro. Uma tubulação levada até um componente colocado se liga a ele pelos conectores (`component_connector_add`, `_set`, `_delete`).
- **Pontos do esboço:** as propriedades de uma peça de esboço listam seus pontos (e o caminho e a seção de uma varredura, o topo de uma transição). Cada coordenada é uma expressão; digite outra, ou prenda-a com ⌖ a um plano visto de perfil na vista do esboço.
- **Varreduras e transições:** as curvas de uma *Varredura* são um caminho aberto; por ele vai uma seção, definida na barra de opções como retângulo (largura × altura) ou círculo (diâmetro), centrada no caminho; os cantos ficam em meia-esquadria. *Transição* pega um contorno de base e depois (após *Concluir*) um de topo, desenhado na mesma vista, e os une; início e fim se definem nas propriedades, como numa extrusão.
- **Seleção e teclas:** clique na linha de um plano, no texto de uma cota, numa aresta de peça ou num conector para selecioná-lo; Delete o remove, e Esc volta passo a passo pelos cliques da ferramenta, pela ferramenta e pela seleção.
- **Concluir e Cancelar:** toda mudança vale para todos os tipos do componente e se desfaz como qualquer edição. *Concluir* mantém as mudanças, *Cancelar edição* desfaz tudo o que foi feito desde que o ambiente abriu.

## Preferências

*Arquivo → Preferências* (Ctrl+,, também no pé da tela inicial) reúne as configurações do próprio aplicativo; elas não fazem parte de nenhum projeto e ficam guardadas entre as sessões. Escolha uma página à esquerda. As mudanças aparecem na hora; **OK** as mantém e **Cancelar** (ou Esc) volta aos valores de quando a janela abriu. **Restaurar padrões** (embaixo, à esquerda) volta as opções da página mostrada aos valores padrão; o idioma e as pastas ficam como estão.

- **Geral**: o idioma da interface, o tema (claro, escuro ou o do sistema) e *Verificar atualizações automaticamente* (veja o aviso de atualização abaixo).
- **Interface**: a *Escala da interface* (75–200%, tudo cresce: painéis, ícones, texto; Ctrl+Mais e Ctrl+Menos também a mudam), o *Tamanho do texto* (pequeno, normal, grande, muito grande; só o texto, em qualquer escala), *Iniciar com o ribbon recolhido*, *Dicas detalhadas* (desligadas, as dicas dos comandos mostram só o nome, o atalho e por que o comando está indisponível, sem a descrição e o comando do motor) e *Arrastar na vista 3D seleciona* (arrastar com o botão esquerdo desenha uma caixa de seleção e Shift+arrastar orbita; desligado, o contrário).
- **Locais de arquivos**: a *Pasta de macros* (os diálogos de arquivo de macro começam nela e *Gerenciar → Executar macro* lista as macros dela), a *Pasta de projetos* (onde começam Abrir, Importar IFC, Importar CAD e o primeiro Salvar de um projeto novo; um projeto já salvo é salvo de novo ao lado dele mesmo) e as pastas da biblioteca de componentes (*Adicionar pasta…*, *Remover*). O botão *Gerenciar pastas…* da janela da biblioteca de componentes abre esta página. Uma pasta que não existe mais aparece como *Não encontrada* e é ignorada.
- **Salvamento**: o intervalo do *Salvamento automático* (desligado, ou a cada 1, 2, 5, 10 ou 15 minutos; veja abaixo) e quantos projetos recentes a tela inicial lista.
- **Teclas de atalho**: todos os comandos com suas teclas. Clique na caixa de um comando e digite duas letras para dar novas teclas a ele (elas substituem as antigas na hora, nas dicas, na paleta e nas vistas); teclas que já são de outro comando são recusadas e a linha diz de qual. Deixe a caixa vazia para tirar as teclas, e *Padrão* traz de volta as originais. A busca acha um comando pelo nome ou pelas teclas. Atalhos com Ctrl, Delete e Esc são fixos.

## Aviso de atualização

O ConstructCAD avisa quando uma versão mais nova é publicada; ele nunca baixa nem instala nada por conta própria, só quando você escolhe **Atualizar agora**.

- **Quando**: ao iniciar, no máximo uma vez por dia, em segundo plano (a janela nunca espera por isso). *Arquivo → Verificar atualizações* (também no diálogo *Sobre*) verifica na hora e informa "está atualizado", a nova versão ou que não foi possível verificar.
- **O que é contatado**: uma requisição HTTPS à API do GitHub, `https://api.github.com/repos/francisreboucas/constructcad/releases/latest`, que devolve a última versão publicada. A requisição envia os dados usuais de conexão (seu endereço IP) e o agente `ConstructCAD/<versão>`; nenhum dado de projeto, identificador ou informação de uso. Vale para ela a declaração de privacidade do GitHub.
- **O que aparece**: quando a versão publicada é mais nova que a em uso (pré-lançamentos são ignorados), um aviso aparece na barra de status e na tela inicial: *Baixar* abre a página da versão no navegador, *Ignorar esta versão* esconde o aviso até sair uma versão posterior, e × o esconde até a próxima verificação. Qualquer falha (sem conexão, tempo esgotado, proxy, limite de requisições) é silenciosa.
- **Atualizar agora**: onde o ConstructCAD pode se substituir com segurança, o aviso e o diálogo da verificação oferecem *Atualizar agora*. Ele baixa o pacote que corresponde à forma como esta cópia foi instalada e confere o SHA-256 com o `SHA256SUMS.txt` da versão (os dois pelos links de download da própria versão no GitHub) antes de mudar qualquer coisa; um download que não confere é apagado e a cópia instalada fica como estava.

  | Instalado como | Atualizar agora faz |
  |---|---|
  | AppImage | Baixa o novo AppImage ao lado do antigo, confere, renomeia por cima do arquivo antigo e oferece *Reiniciar agora* |
  | Instalador do Windows (`.msi`, em Arquivos de Programas) | Baixa e confere o instalador; ele roda quando o ConstructCAD fecha (*Instalar agora* fecha, perguntando antes sobre alterações não salvas; *Ao sair* espera), e o ConstructCAD abre de novo ao terminar |
  | `.zip` do Windows | Baixa e confere o arquivo, coloca o novo `constructcad.exe` e o `ccad.exe` no lugar (os antigos ficam como `.old` e são apagados na próxima abertura) e oferece *Reiniciar agora* |
  | App do macOS | Baixa e confere a imagem de disco e a abre: arraste o ConstructCAD para Aplicativos |
  | `.deb` | Nada: o diálogo mostra o comando do `apt`, com Copiar, e *Baixar* pega o pacote |
  | `.tar.gz`, compilação própria | Nada: *Baixar* abre a página da versão |

  Atualizar agora precisa de permissão de escrita nos arquivos instalados: um AppImage numa pasta sem permissão falha com uma mensagem, e *Baixar* é o caminho para atualizá-lo.
- **Como desligar**: desmarque *Verificar atualizações automaticamente* em *Preferências → Geral*. A variável de ambiente `CONSTRUCTCAD_NO_UPDATE_CHECK=1` desliga toda verificação, automática ou pedida (útil em CI, quiosques e instalações sem rede). O ConstructCAD guarda a hora da última verificação, a versão ignorada e a opção junto com as outras preferências.

## Salvamento automático e recuperação

Um minuto depois que um projeto passa a ter alterações não salvas (também um projeto novo, nunca salvo; *Preferências → Salvamento* define outro intervalo ou desliga o salvamento automático), o ConstructCAD grava uma cópia dele, em segundo plano, na pasta `recovery` da sua pasta de dados (`%APPDATA%\ConstructCAD\data` no Windows, `~/Library/Application Support/ConstructCAD` no macOS, `~/.local/share/constructcad` no Linux). Ele só grava de novo depois que o modelo muda, e nunca sobre o arquivo do projeto. Salvar, fechar o projeto ou sair remove a cópia.

Se o ConstructCAD travar ou for encerrado à força, a próxima abertura mostra *Recuperar trabalho não salvo* com as cópias deixadas: *Restaurar* abre o projeto escolhido com as alterações não salvas (salvá-lo grava no arquivo original, ou pergunta onde, para um projeto nunca salvo), *Descartar* apaga a cópia e *Cancelar* guarda as cópias para a próxima abertura. Uma falha na interface também grava uma última cópia antes de o programa fechar, e toda falha é acrescentada a `crash.log` na pasta de dados; anexe-o ao relatar uma falha.

## Inicialização gráfica

O ConstructCAD desenha com a API gráfica nativa do sistema (DirectX 12 ou Vulkan no Windows, Metal no macOS, Vulkan no Linux) e passa sozinho para o OpenGL quando ela falha, o que algumas GPUs integradas antigas ou pequenas exigem:

- Se a API nativa informar um erro, a mesma abertura tenta o OpenGL. Se ela travar antes de a janela aparecer, o ConstructCAD abre de novo uma vez com o OpenGL; se o processo inteiro morrer (uma falha do driver), a próxima abertura tenta o OpenGL primeiro.
- Depois que o OpenGL foi necessário, ele continua em primeiro: a escolha fica no arquivo `gpu-backend` da pasta de dados. Apague esse arquivo (depois de atualizar o driver de vídeo, por exemplo) para tentar a API nativa de novo.
- Toda abertura que falha é registrada em `crash.log` na pasta de dados. Quando nenhuma API funciona, uma mensagem avisa e indica esse arquivo.
- A variável de ambiente `WGPU_BACKEND` escolhe a API e desliga a troca automática: `gl` para OpenGL, `dx12`, `vulkan` ou `metal`, ou várias separadas por vírgulas (no Windows, por exemplo, `set WGPU_BACKEND=gl` num prompt de comando antes de abrir `constructcad.exe`, ou como variável de ambiente do usuário). `WGPU_POWER_PREF=low` prefere a GPU integrada em computadores com duas.

## Acessibilidade

Os botões do ribbon, os títulos das abas, as setas de rolagem, os itens da paleta, as linhas do navegador, os cartões da tela inicial, os botões de fechar dos diálogos, as seções de propriedades e o botão de início do cubo de vista são desenhados pelo próprio ConstructCAD. Eles informam à interface de acessibilidade do sistema seu papel (botão, item selecionável, cabeçalho recolhível), seu nome e seu estado (habilitado, selecionado, expandido), para que leitores de tela possam anunciá-los.

## Para desenvolvedores

### Adicionar um comando à interface

1. Primeiro, faça da alteração um comando do motor (veja *Para desenvolvedores* no [guia de automação](automacao.md)).
2. Acrescente uma variante a `UiCommand` em `crates/cc_app/src/commands.rs`. Depois preencha `icon`, `label`, `description` (uma chave i18n `cmd-desc-…` em todos os arquivos `.ftl`), `shortcut` (se houver), `engine` (o nome do comando do motor), `badge`, `unavailable` (um motivo traduzido para cada estado em que não pode rodar), `selected` (para ferramentas) e `run`. Inclua-o em `UiCommand::all`.
3. Coloque-o num grupo do ribbon com `command_button` (grande) ou numa família de botões pequenos com `stacks` (`crates/cc_app/src/ribbon.rs`).

A dica, a habilitação, o item da paleta e um atalho de duas letras saem todos do comando. Os testes verificam que cada nome do motor existe no catálogo, que os atalhos não se repetem e que todo comando aparece no ribbon com nome acessível.

### Ícones

Os ícones são arquivos SVG em `assets/icons/ui`, numa grade de 24 unidades. Eles usam traços brancos (`stroke-width` 1.75, pontas e junções arredondadas) e preenchimentos suaves escritos como `fill="#fff" fill-opacity=".2"`. O `icons.rs` separa cada ícone em duas camadas por esses atributos: os traços e as marcas sólidas formam a tinta, e os preenchimentos suaves formam o tom. Cada camada é rasterizada pelo resvg no tamanho exato em pixels e pintada nas cores do tema (`icons::Tones`). Acrescente o novo ícone a `Icon`, `Icon::ALL` e `Icon::source`. `ICON_SHEET=folha.png cargo test -p cc_app icon_sheet` grava todos os ícones em vários tamanhos para revisão.

### Tema

As cores são tokens de `theme::Palette`: `PALETTE` (claro) e `DARK`, com um teste de contraste. Pinte com `theme::palette(ctx)` para que um tema escuro possa ser ligado depois.

### Testes de interface

O `crates/cc_app/src/ui_tests.rs` roda o ribbon e a paleta sem janela, com o [egui_kittest](https://crates.io/crates/egui_kittest). Os testes clicam e digitam pela árvore de acessibilidade, então widgets próprios precisam chamar `Response::widget_info`. As capturas de referência ficam em `crates/cc_app/tests/snapshots`. Elas cobrem cada aba do ribbon, o ribbon estreito e o recolhido, uma dica completa, a paleta, cada página da janela de Preferências e uma galeria de ícones nos dois temas. Placas de vídeo renderizam com pequenas diferenças, então a comparação das capturas roda sob demanda:

```sh
CC_UI_SNAPSHOTS=1 cargo test -p cc_app snapshots     # compara com as referências
UPDATE_SNAPSHOTS=1 cargo test -p cc_app snapshots    # regrava após uma mudança intencional
```

Revise as imagens novas antes do commit. Quando há diferença, o kittest grava `*.new.png` e `*.diff.png` ao lado das referências; o Git ignora esses arquivos.
