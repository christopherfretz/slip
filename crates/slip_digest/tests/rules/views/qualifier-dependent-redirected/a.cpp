#include "dep.h"
namespace legacy { int h(int); }
template <class T> int f(T t) { return legacy::h(t); }
