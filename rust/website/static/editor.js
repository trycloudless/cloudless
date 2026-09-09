// Markdown editor: live preview, image upload with action selection, drag-and-drop
document.addEventListener('DOMContentLoaded', function () {
  var editor = document.getElementById('md-editor');
  var preview = document.getElementById('md-preview');
  var uploadBtn = document.getElementById('upload-btn');
  var imageFile = document.getElementById('image-file');
  var uploadStatus = document.getElementById('upload-status');
  var postUploadActions = document.getElementById('post-upload-actions');
  var insertBtn = document.getElementById('insert-btn');
  var setOgBtn = document.getElementById('set-og-btn');

  if (!editor || !preview) return;

  var lastUploadedUrl = null;
  var lastUploadedAlt = null;

  // --- Live preview (sanitised) ---
  function updatePreview() {
    if (typeof marked !== 'undefined') {
      var raw = marked.parse(editor.value);
      // Sanitise rendered HTML to prevent XSS in the live preview.
      // DOMPurify is loaded from a pinned CDN with SRI in layout.rs.
      if (typeof DOMPurify !== 'undefined') {
        preview.innerHTML = DOMPurify.sanitize(raw);
      } else {
        // Fallback: strip script tags and event handlers
        raw = raw.replace(/<script[\s\S]*?<\/script>/gi, '');
        raw = raw.replace(/\s+on\w+\s*=\s*("[^"]*"|'[^']*'|[^\s>]*)/gi, '');
        preview.innerHTML = raw;
      }
    }
  }
  editor.addEventListener('input', updatePreview);
  updatePreview(); // render initial content (for edit page)

  // --- Insert text at cursor ---
  function insertAtCursor(text) {
    var start = editor.selectionStart;
    var end = editor.selectionEnd;
    var before = editor.value.substring(0, start);
    var after = editor.value.substring(end);
    editor.value = before + text + after;
    var newPos = start + text.length;
    editor.setSelectionRange(newPos, newPos);
    editor.focus();
    updatePreview();
  }

  // --- Image upload via AJAX ---
  // Uploads the file and stores the result; does NOT auto-insert.
  // After success, reveals the Insert / Set as OG Image action buttons.
  function uploadFile(file) {
    if (!file) return;
    uploadStatus.textContent = 'Uploading...';
    if (postUploadActions) postUploadActions.style.display = 'none';

    var formData = new FormData();
    formData.append('file', file);

    fetch('/upload-image', { method: 'POST', body: formData })
      .then(function (resp) {
        if (!resp.ok) throw new Error('Upload failed: ' + resp.statusText);
        return resp.json();
      })
      .then(function (data) {
        lastUploadedUrl = data.url;
        lastUploadedAlt = file.name.replace(/\.[^.]+$/, '');
        uploadStatus.textContent = 'Uploaded: ' + data.url;
        if (imageFile) imageFile.value = '';
        if (postUploadActions) postUploadActions.style.display = 'flex';
      })
      .catch(function (err) {
        uploadStatus.textContent = 'Failed: ' + err.message;
      });
  }

  if (uploadBtn && imageFile) {
    uploadBtn.addEventListener('click', function () {
      var file = imageFile.files[0];
      if (!file) {
        uploadStatus.textContent = 'Select a file first';
        return;
      }
      uploadFile(file);
    });
  }

  // --- Insert into content ---
  if (insertBtn) {
    insertBtn.addEventListener('click', function () {
      if (!lastUploadedUrl) return;
      insertAtCursor('\n![' + lastUploadedAlt + '](' + lastUploadedUrl + ')\n');
      uploadStatus.textContent = 'Inserted!';
    });
  }

  // --- Set as OG Image ---
  if (setOgBtn) {
    setOgBtn.addEventListener('click', function () {
      if (!lastUploadedUrl) return;
      var field = document.getElementById('og-image-field');
      if (!field) return;
      field.value = lastUploadedUrl;

      var ogPreview = document.getElementById('og-image-preview');
      if (ogPreview) ogPreview.src = lastUploadedUrl;

      var ogSet = document.getElementById('og-image-set');
      if (ogSet) ogSet.style.display = 'flex';

      var ogNone = document.getElementById('og-image-none');
      if (ogNone) ogNone.style.display = 'none';

      uploadStatus.textContent = 'Set as OG Image!';
    });
  }

  // --- Drag and drop on textarea ---
  editor.addEventListener('dragover', function (e) {
    e.preventDefault();
    editor.classList.add('ring-2', 'ring-primary');
  });
  editor.addEventListener('dragleave', function () {
    editor.classList.remove('ring-2', 'ring-primary');
  });
  editor.addEventListener('drop', function (e) {
    e.preventDefault();
    editor.classList.remove('ring-2', 'ring-primary');
    var files = e.dataTransfer.files;
    if (files.length > 0 && files[0].type.startsWith('image/')) {
      uploadFile(files[0]);
    }
  });

  // --- Tab key inserts spaces ---
  editor.addEventListener('keydown', function (e) {
    if (e.key === 'Tab') {
      e.preventDefault();
      insertAtCursor('  ');
    }
  });
});

// --- Clear OG Image (called by onclick on the Clear button in the form) ---
window.clearOgImage = function () {
  var field = document.getElementById('og-image-field');
  if (field) field.value = '';

  var ogSet = document.getElementById('og-image-set');
  if (ogSet) ogSet.style.display = 'none';

  var ogNone = document.getElementById('og-image-none');
  if (ogNone) ogNone.style.display = 'block';
};
