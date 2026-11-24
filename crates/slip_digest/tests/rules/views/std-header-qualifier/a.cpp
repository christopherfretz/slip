#include "std.h"
namespace legacy { int h(int); int h(long); }
template <class T> int f(T t) { return legacy::h(t); }
int* g() { return new int; }
