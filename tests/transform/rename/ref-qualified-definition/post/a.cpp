#include "a.h"
int a::g(int x) { return x + 1; }
namespace a {
int u() { return g(1); }
}
int v() { return a::g(2) + ::a::g(3); }
