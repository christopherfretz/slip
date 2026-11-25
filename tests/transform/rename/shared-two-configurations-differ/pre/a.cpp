#include "h.h"
namespace a {
int f(int x) { return x; }
int f(long x) { return (int)x + 1; }
}
int u() { return call(1); }
