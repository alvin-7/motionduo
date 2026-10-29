package app

import (
	"encoding/json"
	"errors"
	"fmt"
	"image/png"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/alvin-7/mousehop/mygo/internal/locale"
	"github.com/alvin-7/mousehop/mygo/internal/model"

	"github.com/egoist/mygo/ui"
)

func TestUnequalResolutionScreensUseEqualSizeThumbnails(t *testing.T) {
	local, remote := &[2]uint32{1920, 1080}, &[2]uint32{1280, 720}
	for _, position := range []string{"left", "right", "top", "bottom"} {
		for _, alignment := range []string{"top_left", "top_right", "bottom_left", "bottom_right"} {
			l, r := screenArrangement(580, 320, local, remote, position, alignment)
			if abs(l.W-r.W) > 0.001 || abs(l.H-r.H) > 0.001 {
				t.Fatalf("resolution affected normalized size: %v %v", l, r)
			}
			if position == "left" || position == "right" {
				end := alignment == "bottom_left" || alignment == "bottom_right"
				if !end && abs(l.Y-r.Y) > 0.001 || end && abs(l.Y+l.H-r.Y-r.H) > 0.001 {
					t.Fatalf("%s/%s: %v %v", position, alignment, l, r)
				}
			} else {
				end := alignment == "top_right" || alignment == "bottom_right"
				if !end && abs(l.X-r.X) > 0.001 || end && abs(l.X+l.W-r.X-r.W) > 0.001 {
					t.Fatalf("%s/%s: %v %v", position, alignment, l, r)
				}
			}
			for _, rect := range []ui.Rect{l, r} {
				if rect.X < 0 || rect.Y < 0 || rect.X+rect.W > 580 || rect.Y+rect.H > 320 {
					t.Fatalf("screen clipped: %v", rect)
				}
			}
		}
	}
}

func TestNormalizedScreensPreserveAspectAndFitEverySide(t *testing.T) {
	for _, pair := range []struct {
		name          string
		local, remote [2]uint32
	}{
		{"win-mac", [2]uint32{5120, 1447}, [2]uint32{1470, 956}},
		{"ultrawide-portrait", [2]uint32{5120, 1447}, [2]uint32{1080, 1920}},
		{"landscape", [2]uint32{1920, 1080}, [2]uint32{1280, 800}},
		{"portrait", [2]uint32{1080, 1920}, [2]uint32{2560, 1440}},
	} {
		t.Run(pair.name, func(t *testing.T) {
			for _, width := range []float32{minWindowWidth - 28, windowWidth - 28} {
				base, _ := screenArrangement(width, layoutStageHeight, &pair.local, &pair.remote, "right", "top_right")
				for _, placement := range screenSnapPlacements {
					l, r := screenArrangement(width, layoutStageHeight, &pair.local, &pair.remote, placement.Position, placement.Alignment)
					if !rectNear(l, base) {
						t.Fatal("changing side moved or resized the local screen", base, l)
					}
					if abs(l.W/l.H-1.6) > 0.001 || abs(r.W/r.H-1.6) > 0.001 {
						t.Fatal("normalization distorted aspect ratio", l, r)
					}
					if placement.Position == "left" || placement.Position == "right" {
						if abs(l.H-r.H) > 0.001 {
							t.Fatal("horizontal neighbors have unequal heights", l, r)
						}
					} else if abs(l.W-r.W) > 0.001 {
						t.Fatal("vertical neighbors have unequal widths", l, r)
					}
					for _, rect := range []ui.Rect{l, r} {
						if rect.W <= 0 || rect.H <= 0 || rect.X < 0 || rect.Y < 0 || rect.X+rect.W > width || rect.Y+rect.H > layoutStageHeight {
							t.Fatal("normalized screen is outside the canvas", placement, rect)
						}
					}
				}
			}
		})
	}
}

func TestThumbnailSizeDoesNotDependOnResolutionOrMissingGeometry(t *testing.T) {
	for _, placement := range screenSnapPlacements {
		a, b := screenArrangement(572, layoutStageHeight, &[2]uint32{1920, 1080}, &[2]uint32{1280, 800}, placement.Position, placement.Alignment)
		c, d := screenArrangement(572, layoutStageHeight, &[2]uint32{3840, 2160}, &[2]uint32{3840, 2400}, placement.Position, placement.Alignment)
		if !rectNear(a, c) || !rectNear(b, d) {
			t.Fatal("pixel density changed the displayed arrangement", a, b, c, d)
		}
		for _, unknown := range []*[2]uint32{nil, {}, {0, 1080}, {1920, 0}} {
			l, r := screenArrangement(572, layoutStageHeight, unknown, &[2]uint32{1470, 956}, placement.Position, placement.Alignment)
			if abs(l.W-r.W) > 0.001 || abs(l.H-r.H) > 0.001 {
				t.Fatal("unknown local geometry did not use the known aspect ratio", l, r)
			}
			l, r = screenArrangement(572, layoutStageHeight, &[2]uint32{1470, 956}, unknown, placement.Position, placement.Alignment)
			if abs(l.W-r.W) > 0.001 || abs(l.H-r.H) > 0.001 {
				t.Fatal("unknown remote geometry did not use the known aspect ratio", l, r)
			}
		}
	}
}

func TestEqualEdgeSnapKeepsBothCornerAnchorsSelectable(t *testing.T) {
	for _, placement := range screenSnapPlacements {
		local, remote := screenArrangement(572, layoutStageHeight, &[2]uint32{1920, 1080}, &[2]uint32{1280, 800}, placement.Position, placement.Alignment)
		moving := remote
		end := placement.Alignment == "bottom_left" || placement.Alignment == "bottom_right"
		if placement.Position == "left" || placement.Position == "right" {
			moving.Y -= 3
			if end {
				moving.Y += 6
			}
		} else {
			end = placement.Alignment == "top_right" || placement.Alignment == "bottom_right"
			moving.X -= 3
			if end {
				moving.X += 6
			}
		}
		snap := nearestScreenSnap(local, moving, screenPlacement{})
		if snap.Placement != placement || !rectNear(snap.Rect, remote) {
			t.Fatal("normalized outline hid a corner anchor", placement, snap)
		}
		snap = nearestScreenSnap(local, remote, placement)
		if snap.Placement != placement {
			t.Fatal("centered pointer changed the existing anchor", placement, snap)
		}
	}
}

func TestUltrawideLayoutNamesSizeAndGap(t *testing.T) {
	for _, remote := range [][2]uint32{{1470, 956}, {1080, 1920}} {
		a := fixture()
		a.page, a.model.Pairing = "Screen layout", nil
		a.model.DeviceName = "Ultrawide workstation"
		a.model.LocalDisplaySize = &[2]uint32{5120, 1447}
		peer := a.model.Clients[7]
		peer.Config.Position, peer.Config.Alignment = "right", "top_right"
		name := "Portrait Mac workstation"
		peer.Config.Hostname = &name
		peer.State.DisplaySize = &remote
		a.model.Clients[7] = peer
		tt := ui.NewTester(a.view, 600, 760)
		l, r := findScreen(t, tt, "Local screen"), findScreen(t, tt, "Remote screen")
		if l.W < 70 || l.H < 20 || abs(r.X-l.X-l.W-screenGap) > 0.1 {
			t.Fatal("unreadable or excessively separated screens", l, r)
		}
		if !tt.HasText(a.model.DeviceName) || !tt.HasText(peer.Name()) {
			t.Fatal("screen names missing", tt.Texts())
		}
		if dir := os.Getenv("MOUSEHOP_TEST_SNAPSHOTS"); dir != "" {
			if err := os.MkdirAll(dir, 0755); err != nil {
				t.Fatal(err)
			}
			f, err := os.Create(filepath.Join(dir, fmt.Sprintf("Screen-layout-ultrawide-%d.png", remote[0])))
			if err != nil {
				t.Fatal(err)
			}
			err = png.Encode(f, tt.Image())
			_ = f.Close()
			if err != nil {
				t.Fatal(err)
			}
		}
		t.Logf("remote %v: local %v remote %v", remote, l, r)
	}
}

func TestEqualEdgeLayoutRenderFixtures(t *testing.T) {
	for _, position := range []string{"right", "top"} {
		a := fixture()
		a.page, a.model.Pairing = "Screen layout", nil
		a.model.LocalDisplaySize = &[2]uint32{1920, 1080}
		peer := a.model.Clients[7]
		peer.Config.Position, peer.Config.Alignment = position, "top_right"
		peer.State.DisplaySize = &[2]uint32{1280, 800}
		a.model.Clients[7] = peer
		tt := ui.NewTester(a.view, windowWidth, windowHeight)
		if dir := os.Getenv("MOUSEHOP_TEST_SNAPSHOTS"); dir != "" {
			if err := os.MkdirAll(dir, 0755); err != nil {
				t.Fatal(err)
			}
			file, err := os.Create(filepath.Join(dir, "Screen-layout-equal-edge-"+position+".png"))
			if err != nil {
				t.Fatal(err)
			}
			err = png.Encode(file, tt.Image())
			_ = file.Close()
			if err != nil {
				t.Fatal(err)
			}
		}
	}
}

func findScreen(t *testing.T, tt *ui.Tester, label string) ui.Rect {
	t.Helper()
	r, ok := tt.Find(label)
	if !ok && label == "Remote screen" {
		r, ok = tt.Find("Remote screen #7")
	}
	if !ok {
		t.Fatalf("missing %s", label)
	}
	return r
}

func TestLayoutTimeoutBannerAppearsWithoutUserInput(t *testing.T) {
	a := fixture()
	a.page, a.model.Pairing = "Screen layout", nil
	tt := ui.NewTester(a.view, 1100, 760)
	peer := a.model.Clients[7]
	a.layoutPending = &pendingPlacement{7, screenPlacement{peer.Config.Position, peer.Config.Alignment}, screenPlacement{"left", "top_left"}, tt.Now().Add(time.Second)}
	tt.Frame()
	tt.Advance(2 * time.Second)
	if !tt.HasText("Screen layout was not confirmed; try again") || a.layoutPending != nil || tt.FramePending() {
		t.Fatal("layout timeout feedback requires another input or keeps polling", tt.Texts())
	}
}

func TestLayoutSaveTimesOutAfterLeavingThePage(t *testing.T) {
	a := fixture()
	a.page, a.model.Pairing = "Screen layout", nil
	tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
	_, point := startLayoutDrag(t, tt, screenPlacement{"left", "top_left"})
	tt.Release(point.X, point.Y)
	if a.layoutPending == nil {
		t.Fatal("drop was not queued")
	}
	a.page = "Devices"
	tt.Frame()
	tt.Advance(4 * time.Second)
	if a.layoutPending != nil || !tt.HasText("Screen layout was not confirmed") || tt.FramePending() {
		t.Fatal("leaving layout hid the deadline or kept polling", tt.Texts())
	}
}

func TestLayoutAcknowledgesOriginalPeerAfterSelectionChanges(t *testing.T) {
	a := fixture()
	a.page, a.model.Pairing = "Screen layout", nil
	second := a.model.Clients[7]
	second.ID, second.Config.Position = 8, "bottom"
	a.model.Clients[8] = second
	tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
	_, point := startLayoutDrag(t, tt, screenPlacement{"left", "top_left"})
	tt.Release(point.X, point.Y)
	a.selected = 8
	tt.Frame()
	peer := a.model.Clients[7]
	peer.Config.Position, peer.Config.Alignment = "left", "top_left"
	data, _ := json.Marshal(map[string]any{"State": []any{peer.ID, peer.Config, peer.State}})
	if _, err := a.receiveEvent(data); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	tt.Advance(4 * time.Second)
	if a.layoutPending != nil || a.model.Error != "" || a.model.Clients[8].Config.Position != "bottom" {
		t.Fatal("acknowledgment used the newly selected peer")
	}
}

func TestDeletedLayoutPeerCannotTransferPendingSaveOrDragToReusedHandle(t *testing.T) {
	for _, pending := range []bool{false, true} {
		t.Run(fmt.Sprint(pending), func(t *testing.T) {
			a := fixture()
			a.page, a.model.Pairing = "Screen layout", nil
			writes := 0
			a.request = func(string, any) error { writes++; return nil }
			tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
			_, point := startLayoutDrag(t, tt, screenPlacement{"left", "bottom_left"})
			if pending {
				tt.Release(point.X, point.Y)
			}
			before := writes
			for _, data := range []string{
				`{"Deleted":7}`,
				`{"Created":[7,{"hostname":"replacement.local","port":4252,"pos":"right","alignment":"top_left"},{"active":true}]}`,
			} {
				if _, err := a.receiveEvent([]byte(data)); err != nil {
					t.Fatal(err)
				}
			}
			tt.Frame()
			tt.Release(point.X, point.Y)
			tt.Advance(4 * time.Second)
			if a.layoutGesture.Active || a.layoutPending != nil || writes != before || a.model.Error != "" {
				t.Fatal("reused handle inherited the old peer's gesture or save")
			}
		})
	}
}

func startLayoutDrag(t *testing.T, tt *ui.Tester, placement screenPlacement) (ui.Rect, ui.Point) {
	t.Helper()
	local, remote := findScreen(t, tt, "Local screen"), findScreen(t, tt, "Remote screen")
	target := placedScreen(local, remote, placement)
	x, y := remote.X+remote.W/2, remote.Y+remote.H/2
	tt.Move(x, y)
	tt.Press(x, y)
	// Deliberately miss the anchor slightly: the magnet, not the pointer,
	// must put the screen exactly on the corner.
	dx, dy := float32(-3), float32(-2)
	if placement.Alignment == "top_right" || placement.Alignment == "bottom_right" {
		dx = -dx
	}
	if placement.Alignment == "bottom_left" || placement.Alignment == "bottom_right" {
		dy = -dy
	}
	point := ui.Point{X: target.X + target.W/2 + dx, Y: target.Y + target.H/2 + dy}
	tt.Move(point.X, point.Y)
	return target, point
}

func rectNear(a, b ui.Rect) bool {
	return abs(a.X-b.X) < 0.1 && abs(a.Y-b.Y) < 0.1 && abs(a.W-b.W) < 0.1 && abs(a.H-b.H) < 0.1
}

func relativeScreen(t *testing.T, tt *ui.Tester, label string) ui.Rect {
	t.Helper()
	r, stage := findScreen(t, tt, label), findScreen(t, tt, "Screen arrangement")
	r.X, r.Y = r.X-stage.X, r.Y-stage.Y
	return r
}

func TestMouseDragMagneticallySnapsEverySideAndCorner(t *testing.T) {
	for _, width := range []int{minWindowWidth, windowWidth} {
		for _, placement := range screenSnapPlacements {
			t.Run(fmt.Sprintf("%d/%s/%s", width, placement.Position, placement.Alignment), func(t *testing.T) {
				a := fixture()
				a.page, a.model.Pairing = "Screen layout", nil
				var calls []string
				a.request = func(kind string, value any) error {
					b, _ := json.Marshal(map[string]any{kind: value})
					calls = append(calls, string(b))
					return nil
				}
				tt := ui.NewTester(a.view, width, minWindowHeight)
				local := findScreen(t, tt, "Local screen")
				target, point := startLayoutDrag(t, tt, placement)
				if len(calls) != 0 {
					t.Fatal("pointer motion must not mutate daemon configuration")
				}
				if !a.layoutGesture.Magnetic || !rectNear(findScreen(t, tt, "Remote screen"), target) {
					t.Fatalf("magnet failed: %#v, target %v", a.layoutGesture, target)
				}
				if !tt.HasText(alignmentLabel(placement.Alignment)) {
					t.Fatal("snap preview has no anchor label")
				}
				tt.Release(point.X, point.Y)
				want := fmt.Sprintf(`{"UpdateScreenPlacement":[7,%q,%q]}`, placement.Position, placement.Alignment)
				if len(calls) != 1 || calls[0] != want {
					t.Fatalf("requests %v, want %s", calls, want)
				}
				if !rectNear(findScreen(t, tt, "Remote screen"), target) || !rectNear(findScreen(t, tt, "Local screen"), local) {
					t.Fatal("drop moved the local screen or lost its pending placement")
				}
				tt.Frame()
				if len(calls) != 1 {
					t.Fatal("a rebuild resubmitted the layout")
				}
			})
		}
	}
}

func TestLayoutDragCancellationDoesNotPersist(t *testing.T) {
	for _, mode := range []string{"escape", "blur", "outside", "offline", "page-change"} {
		t.Run(mode, func(t *testing.T) {
			a := fixture()
			a.page, a.model.Pairing = "Screen layout", nil
			calls := 0
			a.request = func(string, any) error { calls++; return nil }
			tt := ui.NewTester(a.view, 1100, 760)
			_, point := startLayoutDrag(t, tt, screenPlacement{"left", "bottom_left"})
			switch mode {
			case "escape":
				tt.Key(0, ui.KeyEscape)
			case "blur":
				tt.SetFocused(false)
			case "outside":
				point = ui.Point{X: -10, Y: -10}
			case "offline":
				a.model.Disconnect()
				tt.Frame()
			case "page-change":
				a.page = "Devices"
				tt.Frame()
			}
			tt.Release(point.X, point.Y)
			if calls != 0 || a.layoutPending != nil || a.layoutGesture.Active {
				t.Fatal("cancelled gesture persisted or remained active")
			}
		})
	}
}

func TestLayoutDropHandlesQueueFailureTimeoutAndAcknowledgement(t *testing.T) {
	for _, mode := range []string{"send-failure", "timeout", "acknowledged"} {
		t.Run(mode, func(t *testing.T) {
			a := fixture()
			a.page, a.model.Pairing = "Screen layout", nil
			a.request = func(string, any) error {
				if mode == "send-failure" {
					return errors.New("queue full")
				}
				return nil
			}
			tt := ui.NewTester(a.view, 1100, 760)
			original := relativeScreen(t, tt, "Remote screen")
			target, point := startLayoutDrag(t, tt, screenPlacement{"left", "bottom_left"})
			tt.Release(point.X, point.Y)
			if mode == "send-failure" {
				if a.layoutPending != nil || !rectNear(relativeScreen(t, tt, "Remote screen"), original) {
					t.Fatal("failed send left an unsaved optimistic placement")
				}
				return
			}
			if mode == "timeout" {
				tt.Advance(4 * time.Second)
				peer := a.model.Clients[7]
				wantLocal, wantRemote := screenArrangement(a.layoutCanvas.W, a.layoutCanvas.H, a.model.LocalDisplaySize, peer.State.DisplaySize, peer.Config.Position, peer.Config.Alignment)
				// Absolute children start inside the half-DIP stage border.
				wantLocal.X, wantLocal.Y = wantLocal.X+0.5, wantLocal.Y+0.5
				wantRemote.X, wantRemote.Y = wantRemote.X+0.5, wantRemote.Y+0.5
				if a.layoutPending != nil || a.model.Error == "" || !rectNear(relativeScreen(t, tt, "Local screen"), wantLocal) || !rectNear(relativeScreen(t, tt, "Remote screen"), wantRemote) {
					t.Fatal("unconfirmed placement did not revert at the resized error-state canvas", a.layoutPending, a.model.Error, relativeScreen(t, tt, "Local screen"), wantLocal, relativeScreen(t, tt, "Remote screen"), wantRemote)
				}
				return
			}
			peer := a.model.Clients[7]
			peer.Config.Position, peer.Config.Alignment = "left", "bottom_left"
			data, _ := json.Marshal(map[string]any{"State": []any{peer.ID, peer.Config, peer.State}})
			if _, err := a.receiveEvent(data); err != nil {
				t.Fatal(err)
			}
			tt.Frame()
			if a.layoutPending != nil || !rectNear(findScreen(t, tt, "Remote screen"), target) {
				t.Fatal("authoritative placement was not acknowledged")
			}
		})
	}
}

func TestLayoutCanvasResizesAndCancelsDrag(t *testing.T) {
	a := fixture()
	a.page, a.model.Pairing = "Screen layout", nil
	calls := 0
	a.request = func(string, any) error { calls++; return nil }
	tt := ui.NewTester(a.view, 600, 400)
	assertFit := func() {
		t.Helper()
		stage := findScreen(t, tt, "Screen arrangement")
		if abs(stage.W-a.layoutCanvas.W) > 0.1 || abs(stage.H-a.layoutCanvas.H) > 0.1 || stage.Y+stage.H > float32(tt.Image().Bounds().Dy()) {
			t.Fatal("canvas does not match available stage", stage, a.layoutCanvas)
		}
		for _, label := range []string{"Local screen", "Remote screen"} {
			r := relativeScreen(t, tt, label)
			if r.X < 0 || r.Y < 0 || r.X+r.W > stage.W || r.Y+r.H > stage.H-layoutNameHeight {
				t.Fatal("screen overlaps canvas boundary or names", label, r, stage)
			}
		}
		if tt.FramePending() {
			t.Fatal("settled canvas keeps requesting frames")
		}
	}
	assertFit()
	small := a.layoutCanvas
	_, point := startLayoutDrag(t, tt, screenPlacement{"left", "bottom_left"})
	tt.SetSize(1100, 760)
	tt.Release(point.X, point.Y)
	if calls != 0 || a.layoutGesture.Active || a.layoutPending != nil {
		t.Fatal("resize committed an obsolete drag")
	}
	assertFit()
	if a.layoutCanvas.W <= small.W || a.layoutCanvas.H <= small.H {
		t.Fatal("canvas did not grow on both axes", small, a.layoutCanvas)
	}
	large := a.layoutCanvas
	a.model.Error = "Example error"
	tt.Frame()
	assertFit()
	if a.layoutCanvas.H >= large.H {
		t.Fatal("error did not reduce available canvas")
	}
	a.model.Error = ""
	tt.SetSize(600, 400)
	assertFit()
	if !rectNear(a.layoutCanvas, small) {
		t.Fatal("canvas did not return to minimum size", small, a.layoutCanvas)
	}
}

func multiLayoutFixture(count int) *application {
	a := fixture()
	a.page, a.model.Pairing = "Screen layout", nil
	base := a.model.Clients[7]
	for i := 0; i < count; i++ {
		peer := base
		peer.ID = uint64(7 + i)
		name := fmt.Sprintf("Remote computer %d with an exceptionally long workstation name", i+1)
		peer.Config.Hostname = &name
		peer.Config.Position = []string{"right", "top", "left", "bottom", "right"}[i]
		peer.State.Active = i < 4
		a.model.Clients[peer.ID] = peer
	}
	return a
}

func TestVerticalScreensUseSameGapAsHorizontal(t *testing.T) {
	a := multiLayoutFixture(4)
	local, rects, _ := allScreenArrangement(865, 405, a.model.SortedClients(), a.selected, nil)
	for _, peer := range a.model.SortedClients() {
		r := rects[peer.ID]
		var gap float32
		switch peer.Config.Position {
		case "top":
			gap = local.Y - r.Y - r.H
		case "bottom":
			gap = r.Y - local.Y - local.H
		case "left":
			gap = local.X - r.X - r.W
		case "right":
			gap = r.X - local.X - local.W
		}
		if abs(gap-screenGap) > 0.001 {
			t.Fatalf("%s gap = %v, want %v", peer.Config.Position, gap, screenGap)
		}
	}
}

func TestLayoutLocalScreenUsesGenericName(t *testing.T) {
	before := locale.Language
	defer func() { locale.Language = before }()
	locale.Language = "zh-Hans"
	a := multiLayoutFixture(3)
	a.model.DeviceName = "private-hostname"
	tt := ui.NewTester(a.view, 680, 420)
	if !tt.HasText("本机") {
		t.Fatal("local screen does not show the generic localized name", tt.Texts())
	}
}

func TestAllConfiguredScreensFitWithoutOverlap(t *testing.T) {
	for _, count := range []int{3, 4, 5} {
		for _, size := range [][2]int{{600, 400}, {1100, 760}} {
			a := multiLayoutFixture(count)
			tt := ui.NewTester(a.view, size[0], size[1])
			stage := findScreen(t, tt, "Screen arrangement")
			rects := []ui.Rect{relativeScreen(t, tt, "Local screen")}
			for _, peer := range a.model.SortedClients() {
				if !peer.State.Active {
					if tt.HasText(peer.Name()) {
						t.Fatal("disabled client appears in layout", peer.Name())
					}
					continue
				}
				if !tt.HasText(peer.Name()) {
					t.Fatal("missing in-screen name", peer.Name(), tt.Texts())
				}
				r := relativeScreen(t, tt, fmt.Sprintf("Remote screen #%d", peer.ID))
				if r.W <= 0 || r.H <= 0 || abs(r.W/r.H-1.6) > 0.001 || r.X < 0 || r.Y < 0 || r.X+r.W > stage.W || r.Y+r.H > stage.H-layoutNameHeight {
					t.Fatal("screen outside fit", r, stage)
				}
				for _, other := range rects {
					if r.X < other.X+other.W && r.X+r.W > other.X && r.Y < other.Y+other.H && r.Y+r.H > other.Y {
						t.Fatal("overlapping screens", r, other)
					}
				}
				rects = append(rects, r)
			}
			if tt.FramePending() {
				t.Fatal("static multi-screen view keeps requesting frames")
			}
			if dir := os.Getenv("MOUSEHOP_TEST_SNAPSHOTS"); dir != "" {
				if err := os.MkdirAll(dir, 0755); err != nil {
					t.Fatal(err)
				}
				file, err := os.Create(filepath.Join(dir, fmt.Sprintf("Screen-layout-all-%d-%d.png", count, size[0])))
				if err != nil {
					t.Fatal(err)
				}
				err = png.Encode(file, tt.Image())
				_ = file.Close()
				if err != nil {
					t.Fatal(err)
				}
			}
		}
	}
}

func TestClickAndDragDifferentPeerUsesItsOwnID(t *testing.T) {
	a := multiLayoutFixture(3)
	var calls []string
	a.request = func(kind string, value any) error {
		data, _ := json.Marshal(map[string]any{kind: value})
		calls = append(calls, string(data))
		return nil
	}
	tt := ui.NewTester(a.view, 1100, 760)
	remote := findScreen(t, tt, "Remote screen #8")
	x, y := remote.X+remote.W/2, remote.Y+remote.H/2
	tt.Press(x, y)
	if a.selected != 8 || !a.layoutGesture.Active || a.layoutGesture.ID != 8 {
		t.Fatal("clicked peer was not selected")
	}
	target := a.layoutGesture.Targets["bottom"]
	stage := findScreen(t, tt, "Screen arrangement")
	x, y = stage.X+0.5+target.X+target.W/2, stage.Y+0.5+target.Y+target.H/2
	tt.Move(x, y)
	tt.Release(x, y)
	if len(calls) != 1 || calls[0] != `{"UpdateScreenPlacement":[8,"bottom","bottom_left"]}` {
		t.Fatal("drag saved another peer or made non-atomic requests", calls)
	}
	if a.layoutPending == nil || a.layoutPending.ID != 8 {
		t.Fatal("missing originating peer pending save")
	}
	a.selected = 7
	tt.Frame()
	tt.Advance(4 * time.Second)
	if a.layoutPending != nil || a.model.Clients[8].Config.Position != "top" {
		t.Fatal("timeout did not restore authoritative placement")
	}
}

func TestActiveOccupiedSideBlocksSaveWithoutDeactivatingPeer(t *testing.T) {
	for _, language := range []string{"en", "zh-Hans"} {
		t.Run(language, func(t *testing.T) {
			useLanguage(t, language)
			a := multiLayoutFixture(3)
			occupied := a.model.Clients[7]
			occupied.Config.Alias = "Office"
			a.model.Clients[7] = occupied
			writes := 0
			a.request = func(string, any) error { writes++; return nil }
			tt := ui.NewTester(a.view, 1100, 760)
			remote := findScreen(t, tt, fmt.Sprintf("%s #8", tr("Remote screen")))
			tt.Press(remote.X+remote.W/2, remote.Y+remote.H/2)
			target := a.layoutGesture.Targets["right"]
			stage := findScreen(t, tt, tr("Screen arrangement"))
			x, y := stage.X+0.5+target.X+target.W/2, stage.Y+0.5+target.Y+target.H/2
			tt.Move(x, y)
			tt.Release(x, y)
			if writes != 0 || a.layoutPending != nil || !a.model.Clients[7].State.Active || !tt.HasText(trf("Disable %s first", "Office")) {
				t.Fatal("occupied active edge was silently replaced", writes, tt.Texts())
			}
		})
	}
}

func TestMultiScreenPendingPeerDeletionCannotTransferToReusedHandle(t *testing.T) {
	a := multiLayoutFixture(5)
	peer := a.model.Clients[9]
	peer.State.Active = false
	a.model.Clients[9] = peer
	writes := 0
	a.request = func(string, any) error { writes++; return nil }
	tt := ui.NewTester(a.view, 1100, 760)
	remote := findScreen(t, tt, "Remote screen #10")
	tt.Press(remote.X+remote.W/2, remote.Y+remote.H/2)
	target := a.layoutGesture.Targets["left"]
	stage := findScreen(t, tt, "Screen arrangement")
	x, y := stage.X+0.5+target.X+target.W/2, stage.Y+0.5+target.Y+target.H/2
	tt.Move(x, y)
	tt.Release(x, y)
	if a.layoutPending == nil || a.layoutPending.ID != 10 || writes != 1 {
		t.Fatal("peer move did not queue")
	}
	for _, event := range []string{`{"Deleted":10}`, `{"Created":[10,{"hostname":"replacement.local","port":4252,"pos":"right","alignment":"top_left"},{"active":true}]}`} {
		if _, err := a.receiveEvent([]byte(event)); err != nil {
			t.Fatal(err)
		}
	}
	tt.Frame()
	tt.Advance(4 * time.Second)
	if a.layoutPending != nil || a.layoutGesture.Active || a.model.Error != "" || writes != 1 {
		t.Fatal("replacement inherited multi-peer pending state")
	}
}

func TestMultiScreenResizeCancelsDrag(t *testing.T) {
	a := multiLayoutFixture(5)
	writes := 0
	a.request = func(string, any) error { writes++; return nil }
	tt := ui.NewTester(a.view, 1100, 760)
	r := findScreen(t, tt, "Remote screen #10")
	tt.Press(r.X+r.W/2, r.Y+r.H/2)
	tt.Move(r.X-30, r.Y)
	tt.SetSize(600, 400)
	tt.Release(r.X-30, r.Y)
	if writes != 0 || a.layoutGesture.Active || a.layoutPending != nil {
		t.Fatal("resize saved stale multi-screen coordinates")
	}
}

func TestLayoutDisabledPeerRestoresSavedPlacement(t *testing.T) {
	a := fixture()
	a.page, a.model.Pairing = "Screen layout", nil
	tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
	original := relativeScreen(t, tt, "Remote screen")
	peer := a.model.Clients[7]
	placement := peer.Config
	setActive := func(active bool) {
		t.Helper()
		peer.State.Active = active
		data, _ := json.Marshal(map[string]any{"State": []any{peer.ID, peer.Config, peer.State}})
		if _, err := a.receiveEvent(data); err != nil {
			t.Fatal(err)
		}
		tt.Frame()
	}
	setActive(false)
	if _, ok := tt.Find("Remote screen"); ok || a.hasSelection || !tt.HasText("Connect a computer first") {
		t.Fatal("disabled peer remains visible or selected", tt.Texts())
	}
	a.page = "Devices"
	tt.Frame()
	if !tt.HasText(peer.Name()) {
		t.Fatal("disabled peer disappeared from Devices", tt.Texts())
	}
	a.page = "Screen layout"
	setActive(true)
	if a.model.Clients[7].Config.Position != placement.Position || a.model.Clients[7].Config.Alignment != placement.Alignment || !rectNear(relativeScreen(t, tt, "Remote screen"), original) {
		t.Fatal("reenabling did not restore the saved placement")
	}
}

func TestDisablingPeerCancelsLayoutGestureAndPending(t *testing.T) {
	for _, pending := range []bool{false, true} {
		t.Run(fmt.Sprint(pending), func(t *testing.T) {
			a := fixture()
			a.page, a.model.Pairing = "Screen layout", nil
			writes := 0
			a.request = func(string, any) error { writes++; return nil }
			tt := ui.NewTester(a.view, minWindowWidth, minWindowHeight)
			_, point := startLayoutDrag(t, tt, screenPlacement{"left", "top_left"})
			if pending {
				tt.Release(point.X, point.Y)
			}
			before := writes
			peer := a.model.Clients[7]
			peer.State.Active = false
			data, _ := json.Marshal(map[string]any{"State": []any{peer.ID, peer.Config, peer.State}})
			if _, err := a.receiveEvent(data); err != nil {
				t.Fatal(err)
			}
			tt.Frame()
			tt.Release(point.X, point.Y)
			tt.Advance(4 * time.Second)
			if a.hasSelection || a.layoutGesture.Active || a.layoutPending != nil || writes != before || a.model.Error != "" {
				t.Fatal("disabled peer retained gesture, selection or pending save")
			}
		})
	}
}

func TestLayoutDisabledPeerDoesNotAffectRemainingGeometryOrHitTesting(t *testing.T) {
	a := multiLayoutFixture(3)
	peer := a.model.Clients[8]
	peer.State.Active = false
	a.model.Clients[8] = peer
	a.selected, a.hasSelection = 8, true
	tt := ui.NewTester(a.view, 1100, 760)
	if a.selected != 7 || !a.hasSelection || tt.HasText(peer.Name()) {
		t.Fatal("disabled selection was not replaced by an enabled peer")
	}
	clients := []model.Client{a.model.Clients[7], a.model.Clients[9]}
	local, rects, _ := allScreenArrangement(a.layoutCanvas.W, a.layoutCanvas.H, clients, a.selected, nil)
	local.X, local.Y = local.X+0.5, local.Y+0.5
	if !rectNear(relativeScreen(t, tt, "Local screen"), local) {
		t.Fatal("disabled peer affected local geometry")
	}
	for _, client := range clients {
		want := rects[client.ID]
		want.X, want.Y = want.X+0.5, want.Y+0.5
		if !rectNear(relativeScreen(t, tt, fmt.Sprintf("Remote screen #%d", client.ID)), want) {
			t.Fatal("disabled peer affected remote geometry", client.ID)
		}
	}
	if _, ok := tt.Find("Remote screen #8"); ok {
		t.Fatal("disabled peer retained a hit target")
	}
}

func TestNewRequestSurfacesWindowOnlyOnce(t *testing.T) {
	a := newApplication()
	data := []byte(`{"PairingRequests":[{"id":"opaque","name":"Office PC","addresses":[]}]}`)
	show, err := a.receiveEvent(data)
	if err != nil || !show {
		t.Fatalf("new request: %v %v", show, err)
	}
	show, err = a.receiveEvent(data)
	if err != nil || show {
		t.Fatalf("duplicate request steals focus: %v %v", show, err)
	}
	show, err = a.receiveEvent([]byte(`{"CaptureStatus":"Enabled"}`))
	if err != nil || show {
		t.Fatalf("unrelated event steals focus: %v %v", show, err)
	}
}
