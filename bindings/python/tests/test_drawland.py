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
