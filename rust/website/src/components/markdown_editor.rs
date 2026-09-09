use leptos::prelude::*;

/// Split-pane markdown editor with live preview and image upload.
///
/// Renders a textarea (left) and preview div (right). All interactivity
/// (preview rendering, image upload, drag-drop) is handled by `/static/editor.js`
/// which must be loaded on the page via the Layout `editor_mode` prop.
#[component]
pub fn MarkdownEditor(
    /// Initial markdown content (empty for create, existing content for edit).
    #[prop(default = String::new())]
    initial_content: String,
) -> impl IntoView {
    view! {
        <div class="flex flex-col lg:flex-row gap-4" id="editor-container">
            // Left pane: Markdown input
            <div class="flex-1 min-w-0">
                <label class="block text-sm font-medium text-text-secondary mb-2">"Content (Markdown)"</label>
                <textarea
                    id="md-editor"
                    name="content"
                    rows="25"
                    placeholder="# My Blog Post\n\nWrite your content in **markdown** here..."
                    class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary font-mono text-sm focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all resize-y"
                    required
                >{initial_content}</textarea>

                // Image upload controls
                <div class="mt-3 flex items-center gap-3 flex-wrap">
                    <input
                        type="file"
                        id="image-file"
                        accept="image/*"
                        class="text-sm text-text-secondary file:mr-3 file:py-2 file:px-4 file:rounded-lg file:border-0 file:text-sm file:font-medium file:bg-primary-tint file:text-primary-light hover:file:bg-primary-tint file:cursor-pointer file:transition-all"
                    />
                    <button
                        type="button"
                        id="upload-btn"
                        class="border border-border text-text-secondary px-4 py-2 rounded-lg text-sm hover:bg-surface transition-all whitespace-nowrap"
                    >
                        "Upload"
                    </button>
                    <span id="upload-status" class="text-xs text-text-secondary"></span>
                </div>
                // Post-upload action buttons — hidden until an upload succeeds
                <div id="post-upload-actions" class="mt-2 flex items-center gap-2" style="display:none">
                    <button
                        type="button"
                        id="insert-btn"
                        class="border border-border text-text-secondary px-3 py-1.5 rounded-lg text-xs hover:bg-surface transition-all whitespace-nowrap"
                    >
                        "Insert into Content"
                    </button>
                    <button
                        type="button"
                        id="set-og-btn"
                        class="border border-primary-glow text-primary-light px-3 py-1.5 rounded-lg text-xs hover:bg-primary-tint transition-all whitespace-nowrap"
                    >
                        "Set as OG Image"
                    </button>
                </div>
                <p class="text-xs text-text-secondary mt-1">"Drag and drop images onto the editor, or use the upload button."</p>
            </div>

            // Right pane: Live preview
            <div class="flex-1 min-w-0">
                <label class="block text-sm font-medium text-text-secondary mb-2">"Preview"</label>
                <div
                    id="md-preview"
                    class="w-full bg-surface-alpha border border-border rounded-xl px-6 py-4 min-h-[600px] overflow-y-auto
                        prose prose-invert prose-lg max-w-none
                        [&_h2]:text-2xl [&_h2]:font-display [&_h2]:font-bold [&_h2]:text-text-primary [&_h2]:mt-10 [&_h2]:mb-4
                        [&_h3]:text-xl [&_h3]:font-semibold [&_h3]:text-text-primary [&_h3]:mt-8 [&_h3]:mb-3
                        [&_p]:text-text-secondary [&_p]:leading-relaxed [&_p]:mb-4
                        [&_a]:text-primary-light [&_a]:hover:text-text-primary [&_a]:transition-colors
                        [&_ul]:text-text-secondary [&_ul]:space-y-2 [&_ul]:my-4 [&_ul]:list-disc [&_ul]:pl-6
                        [&_ol]:text-text-secondary [&_ol]:space-y-2 [&_ol]:my-4 [&_ol]:list-decimal [&_ol]:pl-6
                        [&_code]:bg-surface [&_code]:px-2 [&_code]:py-0.5 [&_code]:rounded [&_code]:text-sm [&_code]:text-accent-light
                        [&_pre]:bg-surface [&_pre]:border [&_pre]:border-border [&_pre]:rounded-xl [&_pre]:p-4 [&_pre]:overflow-x-auto
                        [&_blockquote]:border-l-4 [&_blockquote]:border-primary [&_blockquote]:pl-4 [&_blockquote]:text-text-secondary [&_blockquote]:italic
                        [&_img]:rounded-xl [&_img]:max-w-full"
                >
                    <p class="text-text-secondary italic">"Start typing to see preview..."</p>
                </div>
            </div>
        </div>
    }
}
