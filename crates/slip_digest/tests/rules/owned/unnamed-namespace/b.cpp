namespace acme { namespace { struct Hidden {}; } struct Mine { Hidden h; }; }
int use() { return sizeof(acme::Mine); }
