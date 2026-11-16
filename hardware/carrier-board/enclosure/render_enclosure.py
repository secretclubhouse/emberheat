#!/usr/bin/env python3
"""Shaded renders of the enclosure for the owner: renders/enclosure-closed.png,
-open.png, -front.png and -back.png, 2400 px wide on white.

The renders show the real parts: the model's board frame is left-handed, so
every solid is mirrored in y first (export_enclosure.real_part), as for the
exported files. In the front view the zone terminals are then at the top, the
power input notch on the left, and the numerals read 1 to 7 left to right.

Colours: body light grey, lettering black, pane translucent, board green and
its parts grey boxes. Offscreen VTK (it ships with CadQuery).
Run with the CadQuery environment: make enclosure."""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
RENDERS = os.path.join(os.path.dirname(HERE), "renders")

WIDTH, HEIGHT = 2400, 1600
TOL, ANGLE = 0.05, 0.3        # tessellation for display

BODY = (0.80, 0.80, 0.78)
STRIP = (0.88, 0.88, 0.86)
LETTERS = (0.02, 0.02, 0.02)
PANE = (0.55, 0.75, 0.90)
BOARD = (0.05, 0.45, 0.20)
PART = (0.55, 0.55, 0.57)


def poly_data(shape):
    """A vtkPolyData of the shape's surface, with normals."""
    from vtkmodules.vtkCommonCore import vtkPoints
    from vtkmodules.vtkCommonDataModel import vtkCellArray, vtkPolyData
    from vtkmodules.vtkFiltersCore import vtkPolyDataNormals
    verts, tris = shape.tessellate(TOL, ANGLE)
    pts = vtkPoints()
    for v in verts:
        pts.InsertNextPoint(v.x, v.y, v.z)
    cells = vtkCellArray()
    for a, b, c in tris:
        cells.InsertNextCell(3, (a, b, c))
    pd = vtkPolyData()
    pd.SetPoints(pts)
    pd.SetPolys(cells)
    nm = vtkPolyDataNormals()
    nm.SetInputData(pd)
    nm.SetFeatureAngle(35)
    nm.SplittingOn()
    nm.ConsistencyOn()
    nm.Update()
    return nm.GetOutput()


def actor(shape, color, opacity=1.0, edges=False):
    """Actors for a shape (the surface, and optionally its sharp edges)."""
    from vtkmodules.vtkFiltersCore import vtkFeatureEdges
    from vtkmodules.vtkRenderingCore import vtkActor, vtkPolyDataMapper
    # Pull edge lines toward the viewer so they are not buried in the surface.
    vtkPolyDataMapper.SetResolveCoincidentTopologyLineOffsetParameters(-3, -3)
    pd = poly_data(shape)
    m = vtkPolyDataMapper()
    m.SetInputData(pd)
    m.SetResolveCoincidentTopologyToPolygonOffset()
    a = vtkActor()
    a.SetMapper(m)
    pr = a.GetProperty()
    pr.SetColor(*color)
    pr.SetOpacity(opacity)
    pr.SetAmbient(0.25)
    pr.SetDiffuse(0.75)
    pr.SetSpecular(0.15)
    out = [a]
    if edges:
        # tessellate() gives each B-rep face its own points; merge them so
        # the angle between faces can be measured.
        from vtkmodules.vtkFiltersCore import vtkCleanPolyData
        cl = vtkCleanPolyData()
        cl.SetInputData(pd)
        cl.SetTolerance(1e-6)
        cl.PointMergingOn()
        cl.Update()
        fe = vtkFeatureEdges()
        fe.SetInputData(cl.GetOutput())
        fe.BoundaryEdgesOff()
        fe.NonManifoldEdgesOff()
        fe.FeatureEdgesOn()
        fe.SetFeatureAngle(40)
        fe.Update()
        em = vtkPolyDataMapper()
        em.SetInputData(fe.GetOutput())
        em.ScalarVisibilityOff()
        em.SetResolveCoincidentTopologyToPolygonOffset()
        ea = vtkActor()
        ea.SetMapper(em)
        ea.GetProperty().SetColor(0.25, 0.25, 0.25)
        ea.GetProperty().SetLineWidth(1.5)
        ea.GetProperty().SetLighting(False)
        out.append(ea)
    return out


def real_parts(geom, p):
    """Every part as a physical-frame shape, assembled."""
    import export_enclosure
    import gen_enclosure
    built = gen_enclosure.build(geom, p)
    return {k: export_enclosure.real_part(v.val()) for k, v in built.items()}


def real_board(geom, p):
    """(board shape, [part shapes]) in the physical frame."""
    import board_model
    import export_enclosure
    solids = board_model.board_solids(geom, p)
    parts = [export_enclosure.real_part(s.val()) for ref, s in solids.items()
             if ref != "board" and ref != "usb_plug" and not ref.startswith(("antenna_", "plug_"))]
    return export_enclosure.real_part(solids["board"].val()), parts


def render(actors, path, direction, up=(0, 1, 0), parallel=False, zoom=1.0, shift=(0.0, 0.0)):
    """Render the actors looking along `direction` (from the viewer into the
    scene) to a PNG, framed on their bounding box. shift moves the frame by
    that fraction of the window."""
    import vtkmodules.vtkRenderingOpenGL2  # noqa: F401  (registers the OpenGL backend)
    import vtkmodules.vtkInteractionStyle  # noqa: F401
    from vtkmodules.vtkIOImage import vtkPNGWriter
    from vtkmodules.vtkRenderingCore import vtkLightKit, vtkRenderer, vtkRenderWindow, vtkWindowToImageFilter
    ren = vtkRenderer()
    ren.SetBackground(1, 1, 1)
    ren.SetUseDepthPeeling(1)
    ren.SetMaximumNumberOfPeels(8)
    ren.SetOcclusionRatio(0.0)
    for a in actors:
        ren.AddActor(a)
    kit = vtkLightKit()
    kit.AddLightsToRenderer(ren)
    win = vtkRenderWindow()
    win.SetOffScreenRendering(1)
    win.SetMultiSamples(0)
    win.SetAlphaBitPlanes(1)
    win.SetSize(WIDTH, HEIGHT)
    win.AddRenderer(ren)
    ren.ResetCamera()
    cam = ren.GetActiveCamera()
    fp = cam.GetFocalPoint()
    d = [c / sum(x * x for x in direction) ** 0.5 for c in direction]
    dist = cam.GetDistance()
    cam.SetPosition(fp[0] - d[0] * dist, fp[1] - d[1] * dist, fp[2] - d[2] * dist)
    cam.SetViewUp(*up)
    cam.SetParallelProjection(1 if parallel else 0)
    ren.ResetCamera()
    cam.Zoom(zoom)
    if shift != (0.0, 0.0):
        fp = cam.GetFocalPoint()
        pos = cam.GetPosition()
        # window units -> world: use the view's right and up vectors
        import numpy as np
        dv = np.array(fp) - np.array(pos)
        right = np.cross(dv, up)
        right = right / np.linalg.norm(right)
        upv = np.cross(right, dv)
        upv = upv / np.linalg.norm(upv)
        scale = cam.GetParallelScale() * 2 if parallel else np.linalg.norm(dv) * 0.9
        off = right * shift[0] * scale * WIDTH / HEIGHT + upv * shift[1] * scale
        cam.SetFocalPoint(*(np.array(fp) + off))
        cam.SetPosition(*(np.array(pos) + off))
    ren.ResetCameraClippingRange()
    win.Render()
    w2i = vtkWindowToImageFilter()
    w2i.SetInput(win)
    w2i.SetInputBufferTypeToRGB()
    w2i.ReadFrontBufferOff()
    w2i.Update()
    os.makedirs(os.path.dirname(path), exist_ok=True)
    wr = vtkPNGWriter()
    wr.SetFileName(path)
    wr.SetInputConnection(w2i.GetOutputPort())
    wr.Write()
    print("wrote %s" % path)


def closed_actors(parts):
    return (actor(parts["base"], BODY, edges=True) + actor(parts["cover"], BODY, edges=True)
            + actor(parts["dial_strip"], STRIP, edges=True) + actor(parts["dial_lettering"], LETTERS)
            + actor(parts["pane"], PANE, opacity=0.35))


def main():
    import board_model
    from params import P
    geom = board_model.load_geometry()
    parts = real_parts(geom, P)
    board, board_parts = real_board(geom, P)

    # The real part: x right, y up, z toward the viewer. Looking along -z is
    # the front; along +z is the back.
    front, back = (0, 0, -1), (0, 0, 1)
    iso = (0.55, -0.45, -0.7)            # from the front, upper left

    render(closed_actors(parts), os.path.join(RENDERS, "enclosure-closed.png"), iso, zoom=1.25)

    open_actors = actor(parts["base"], BODY, edges=True) + actor(board, BOARD)
    for s in board_parts:
        open_actors += actor(s, PART)
    render(open_actors, os.path.join(RENDERS, "enclosure-open.png"), iso, zoom=1.25)

    render(closed_actors(parts), os.path.join(RENDERS, "enclosure-front.png"), front, parallel=True, zoom=1.35)

    back_actors = actor(parts["base"], BODY, edges=True)
    render(back_actors, os.path.join(RENDERS, "enclosure-back.png"), back, parallel=True, zoom=1.35)


if __name__ == "__main__":
    main()
