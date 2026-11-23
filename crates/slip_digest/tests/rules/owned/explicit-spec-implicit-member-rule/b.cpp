#include "dep.h"
namespace acme { struct Mine {}; }
template <> struct legacy::box<acme::Mine> { int get() { return 1; } };
int use() { legacy::box<acme::Mine> b; return b.get(); }
