#include "old_dep.h"
#include "new_dep.h"
namespace acme { int g(int); using legacy::h; }
template <class T> int f(T t) { return acme::h(t); }
