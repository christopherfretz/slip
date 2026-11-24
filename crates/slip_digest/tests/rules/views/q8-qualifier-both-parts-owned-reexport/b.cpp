#include "old_dep.h"
#include "new_dep.h"
namespace acme { int k(int); using legacy::k; }
template <class T> int f(T t) { return acme::k(t); }
