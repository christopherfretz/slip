#include "dep.h"
namespace acme { struct Mine {}; }
template <class T> struct legacy::box<T *> { int get() { return 2; } };
int use() { legacy::box<acme::Mine *> b; return b.get(); }
