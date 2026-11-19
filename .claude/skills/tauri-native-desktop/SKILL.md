---
name: tauri-native-desktop
description: "Audit and improvement of the desktop integration of Tauri apps: OS windows and dialogs, Windows/macOS/Linux conventions, accessibility and lifecycle. Use it to make Tauri more native, not for web restyling alone or for mobile apps."
---

# Tauri Native Desktop

Act as a desktop integration engineer. Optimize the behaviour that can be observed on the chosen operating system, keeping the project's architecture and permissions. Looking alike does not replace correct focus, keyboard, accessibility and lifecycle.

## Identify the level required

Always distinguish:

- **Native OS control**: file picker, message box, menu, tray; it uses the system's APIs, directly or through a verified plugin.
- **Native window with web content**: a separate Tauri window managed by the window manager; the form inside is still HTML/CSS.
- **Fully native interface**: AppKit, WinUI/Win32 or GTK widgets; it needs a specific integration or a different choice of toolkit. Tauri does not convert React into these controls by itself.

For forms outside the main window the second level is often the right one. If native widgets are needed too, state the cost and verify the integration before promising it.

## Procedure

1. **Read the context.** Read the local instructions, manifests and lockfiles. Identify the platforms shipped, the Tauri/plugin/WebView versions, the windows, any shared web frontend, authentication, application state and ACLs. Ask only what changes the solution and cannot be found in the project.
2. **Classify the surfaces.** List dialogs, panels, menus and notices. Choose for each one an OS control, a Tauri window or an internal surface. Record the reason and the fallback. A request for external dialogs does not turn tooltips and dropdown menus into windows.
3. **Define the desktop contract.** Read [the guide](references/guide.md) and [the checklist](references/acceptance.md). Define owner, modality, sizing, focus, closing, result and authoritative state. Verify on the versions in use: `parent`, `owner`, `transient_for`, sheet and modality are not synonyms.
4. **Build the authorized scope.** In an audit deliver evidence and priorities. When changes are requested, implement verifiable increments that reuse the existing domain and transport. Define the least privileges of the new windows and test the caller. Keep the browser path when the product needs it.
5. **Verify in the real app.** Run the relevant automated checks and try the installed app on the declared platforms. Distinguish code inspected, compilation, automated tests and behaviour observed. Report the checks that still need a different environment.

## Essential decisions

- Prefer the OS frame when native behaviour is the priority and the project does not need a bar of its own. If you keep the custom bar, verify the system functions it replaces.
- A modal dialog needs the main window restored reliably on confirm, cancel, X, error and destruction. `always_on_top` does not fulfil this contract.
- Converting a dialog into an OS window moves its whole lifecycle too: opening, availability, confirm/save, cancel, Escape, X/Alt+F4, closing of the owner, error and shutdown. A successful save must close the OS window, not only unmount the React content: an empty window is a defect. If the save fails, the draft, the message and the chance to retry stay. Verify also that modality is released, focus returns and cleanup runs once; do not assume that `window.close()`, `close()`, `destroy()` and the React unmount follow the same path in the chosen runtime.
- A form cut by the frame of the main window needs a suitable external surface. Try small windows, enlarged text and long content: a scrolling body and reachable actions, without depending on the edges of the main window.
- Another WebView has a separate JavaScript context. Use the service/host that is already authoritative, a reliable first read and targeted invalidation events. Avoid a second backend and secrets in URLs. If only main reaches the credentials, define the authenticated path of the secondary window before implementing it; the guide compares relay and broker.
- Apply the relevant desktop bootstrap to every new WebView, not only to main. A Settings window can be non-modal and reusable; being external does not require blocking the main window.
- Keep editing, useful selection, IME and accessible zoom while you remove the browser commands that do not belong. Replace intercepted shortcuts explicitly.
- Assess bounce explicitly: distinguish overscroll/rubber-banding from elastic animations. Choose per surface and platform, preserving scrolling and motion preferences; the guide has criteria and targeted CSS.
- Mica/blur, tray, autostart, global shortcuts and updater are product choices, not a package required to be native.

## Delivery

Show first the behaviour obtained or the priority defect. Include files and lines, platform/version, checks and limits. For an audit use `surface → current state → solution → acceptance test`. One concrete final action if work is left to the user.

The primary sources in the guide describe the research of 2026-11-11. Check the APIs needed again for each new project, above all modality, notifications and permissions. The skill provides competence: it does not by itself authorize changes, installations or publications.
