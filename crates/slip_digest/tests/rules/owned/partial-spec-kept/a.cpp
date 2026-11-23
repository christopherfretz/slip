#include "dep.h"
namespace legacy { struct Mine {}; }
template <class T> struct legacy::box<T *> { int get() { return 2; } };
int use() { legacy::box<legacy::Mine *> b; return b.get(); }
