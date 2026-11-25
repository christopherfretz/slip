#include "a.h"
int a::f(int x) { return x + 1; }
namespace a {
int u() { return f(1); }
}
int v() { return a::f(2) + ::a::f(3); }
