// §10.7 (WP-033 review, item 7): a skipped `export module` line makes Kiln track ownership; the translation unit is no module unit and writes no owner.
#if 0
export module m;
#endif
int x;
