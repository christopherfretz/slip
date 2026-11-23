#include "dep.h"
namespace legacy { struct Mine {}; }
template <> struct legacy::box<legacy::Mine> { int get() { return 0; } };
int use() { legacy::box<legacy::Mine> b; return b.get(); }
