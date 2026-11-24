#include "new_dep.h"
namespace acme { int g(int); }
template <class T> int f(T t) { return acme::d(t); }
