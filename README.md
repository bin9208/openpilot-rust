# Verified CI helper distribution for #296

This branch distributes four canonical CI helper files without the unrelated runtime tree. Actions callers pin the immutable distribution commit.

The helper bytes are unchanged from `77663836a95a9e2aff9aa66592d8b06ed9c87cd9`. `tools/ci/helper-provenance.json` records original Git blobs, SHA256 values and the existing bounded source validation. The original repository license is retained.

The tree contains CI helpers, provenance and license documentation. It contains no workflows, runtime or model assets. Distribution loading and each changed caller require their own Actions evidence; the source validation is not full runtime or physical ARM acceptance.
