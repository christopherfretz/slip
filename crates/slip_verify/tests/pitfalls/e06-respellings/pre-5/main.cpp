using Int = int;
using Integer = int;
int plain(Int v) { return v; }
const char *name(Int) { return __PRETTY_FUNCTION__; }
int width(Integer) { return sizeof(__PRETTY_FUNCTION__); }
