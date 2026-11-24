#include "new_dep.h"
namespace legacy { int h(long); }
namespace acme { using legacy::h; }
template <class T> int f(T t) { return acme::h(t); }
