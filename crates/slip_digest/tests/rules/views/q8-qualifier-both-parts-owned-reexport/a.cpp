#include "old_dep.h"
#include "new_dep.h"
namespace legacy { int k(int); }
template <class T> int f(T t) { return legacy::k(t); }
