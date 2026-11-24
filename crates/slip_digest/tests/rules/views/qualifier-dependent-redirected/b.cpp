#include "dep.h"
namespace acme { int h(int); }
template <class T> int f(T t) { return acme::h(t); }
