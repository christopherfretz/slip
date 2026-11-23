namespace legacy { namespace { struct Hidden {}; } struct Mine { Hidden h; }; }
int use() { return sizeof(legacy::Mine); }
