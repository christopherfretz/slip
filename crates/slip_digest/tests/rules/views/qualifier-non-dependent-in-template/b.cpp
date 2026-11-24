#include "dep.h"
namespace acme { int h(int); }
template <class T> int f(T) { return acme::h(1); }
