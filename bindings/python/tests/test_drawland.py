import drawland


def test_postcard_graph_needs_no_network():
    g = drawland.graph(lat=45.0, lng=9.0)
    assert g["output"]["lat"] == 45.0
    assert g["draw"]


def test_script_text_and_views_by_name(tmp_path):
    (tmp_path / "lines.js").write_text("output({ scale: 2000 })\ndraw({ id: 'roads', from: lines.of(features.tiles('roads')) })\n")
    page = "output({ lat: 45, lng: 9 })\nview({ id: 'a', at: [0, 0], size: [50, 40], lat: 41.4, lng: 2.17, scale: '1:4000' }, 'lines')\n"
    g = drawland.graph(script=page, dir=tmp_path)
    assert g["views"][0]["id"] == "a"


def test_a_script_error_says_where():
    try:
        drawland.graph(script="areas.nope()")
    except RuntimeError as e:
        assert "line 1" in str(e)
    else:
        raise AssertionError("no error")


def test_the_cli_answers():
    assert drawland.cli(["--version"]) == 0


def test_a_graph_built_in_python_is_the_scripts():
    js = """output({ lat: 37.802, lng: -122.263, width: "6in", height: "4in", scale: "1:8000" })
const water = areas.polygons(features.tiles("water"))
const b = areas.polygons(features.tiles("buildings"))
draw({ id: "water", from: fills.fill({ in: water, pattern: "waves", spacing: "0.8mm" }) })
draw({ id: "buildings", from: lines.rings(b, { exteriorOnly: true }) })
compose.mask({ from: b, occludes: ["water"] })
"""
    g = drawland.Graph()
    g.output(lat=37.802, lng=-122.263, width="6in", height="4in", scale="1:8000")
    water = g.areas.polygons(g.features.tiles("water"))
    b = g.areas.polygons(g.features.tiles("buildings"))
    g.draw(id="water", from_=g.fills.fill(in_=water, pattern="waves", spacing="0.8mm"))
    g.draw(id="buildings", from_=g.lines.rings(b, exteriorOnly=True))
    g.compose.mask(from_=b, occludes=["water"])

    def strip(v):
        if isinstance(v, dict):
            return {k: strip(x) for k, x in v.items() if k != "at"}
        if isinstance(v, list):
            return [strip(x) for x in v]
        return v

    assert strip(g.to_dict()) == strip(drawland.graph(script=js))


def test_python_functions_for_views_and_remaps():
    g = drawland.Graph()
    g.output(lat=45.0, lng=9.0, scale="1:20000", warp={"remap": lambda p: [p[0], p[1] * 0.5], "grid": 4})
    assert g.page.width > 0 and abs(g.world.center["lat"] - 45.0) < 1e-6

    def inset(v):
        v.draw(id="roads", from_=v.lines.of(v.features.tiles("roads")))

    assert g.view(inset, id="inset", at=[0, 0], size=[40, 30], lat=41.4, lng=2.17, scale="1:4000") == "inset"
    labels = g.modules.labels(from_=g.features.tiles("roads"), keys="name", rules=[{"tags": {"highway": "primary"}, "id": "big"}])
    g.draw(id="labels", from_=labels.lines)
    g.compose.mask(from_=labels.big.area, occludes=["labels"])
    d = g.to_dict()
    assert len(d["views"]) == 1
    assert "mesh" in d["output"]["warp"][0]


def test_python_names_the_same_mistakes():
    g = drawland.Graph()
    try:
        g.areas.union(1)
    except drawland.Error as e:
        assert "areas.union expects an area" in str(e)
    else:
        raise AssertionError("no error")


def test_a_view_in_a_javascript_style_by_name(tmp_path):
    (tmp_path / "lines.js").write_text("output({ scale: 2000 })\ndraw({ id: 'roads', from: lines.of(features.tiles('roads')) })\n")
    g = drawland.Graph(dir=tmp_path)
    g.output(lat=45.0, lng=9.0)
    g.view("lines", id="a", at=[0, 0], size=[50, 40], lat=41.4, lng=2.17, scale="1:4000")
    assert g.to_dict()["views"][0]["id"] == "a"


def test_typescript(tmp_path):
    ts = "const s: string = '0.5mm'\noutput({ lat: 45, lng: 9 })\ndraw({ id: 'r', from: lines.of(features.tiles('roads')) as Lines })\n"
    assert drawland.graph(script=ts, typescript=True)["draw"]
    (tmp_path / "p.ts").write_text(ts)
    assert drawland.graph(tmp_path / "p.ts")["draw"]
