#include "old_dep.h"
#include "new_dep.h"
namespace legacy { int g(int); }
template <class T> int f(T t) { return legacy::h(t); }
