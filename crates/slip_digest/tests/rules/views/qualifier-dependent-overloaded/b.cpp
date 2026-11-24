#include "dep.h"
namespace acme { int h(int); int h(long); }
template <class T> int f(T t) { return acme::h(t); }
