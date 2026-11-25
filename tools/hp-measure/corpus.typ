#let kind = sys.inputs.at("kind")
#let source = json("content.json")
#set page(paper: "us-letter", margin: 0.85in)
#set text(font: "Liberation Serif", size: 11pt, lang: "en", ligatures: true)

#if kind == "paper" [
  #set page(columns: 2)
  #set par(justify: true)
  = #source.paper.title
  #emph(source.paper.author)

  == Observations
  #source.paper.left#footnote(source.paper.note)

  #colbreak()
  == Comparison
  #source.paper.right
] else if kind == "report" [
  #set text(font: "Liberation Sans")
  = #source.report.title
  #source.report.opening

  #table(
    columns: (2fr, 1fr, 2fr),
    inset: 9pt,
    table.header(..source.report.headers.map(strong)),
    ..source.report.rows.flatten(),
  )

  == Next steps
  #source.report.closing
] else if kind == "book" {
  for number in range(1, 301) {
    if number > 1 { pagebreak() }
    heading(level: 1, source.book.title)
    emph("Leaf " + str(number))
    parbreak()
    for paragraph in source.book.body {
      text(paragraph)
      parbreak()
    }
  }
} else if kind == "chinese" [
  #set text(font: "Noto Sans CJK SC", size: 19pt, lang: "zh")
  #for line in source.chinese [#line #parbreak()]
] else if kind == "vertical" {
  set text(font: "Noto Sans CJK SC", size: 21pt, lang: "zh")
  // Place upright glyphs top to bottom, with the reading order running right to left.
  place(top + right, grid(
    columns: source.vertical.len(),
    column-gutter: 23pt,
    ..source.vertical.rev().map(line => stack(dir: ttb, spacing: 3pt, ..line.clusters())),
  ))
}
