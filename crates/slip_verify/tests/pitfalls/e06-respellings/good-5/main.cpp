using Int = int;
using Integer = int;
int plain(int v) { return v; }
const char *name(int) { return __PRETTY_FUNCTION__; }
int width(Integer) { return sizeof(__PRETTY_FUNCTION__); }
