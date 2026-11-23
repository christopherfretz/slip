#include "dep.h"
namespace legacy { struct Mine {}; }
template <> struct legacy::box<legacy::Mine> { int get() { return 1; } };
int use() { legacy::box<legacy::Mine> b; return b.get(); }
