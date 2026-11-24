#include "dep.h"
namespace legacy { int h(int); }
template <class T> int f(T) { return legacy::h(1); }
