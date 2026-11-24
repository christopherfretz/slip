#include "new_dep.h"
namespace legacy { int g(int); }
template <class T> int f(T t) { return acme::d(t); }
