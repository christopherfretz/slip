#include "cp.h"
namespace a {
struct S {};
int f(S) { return 1; }
}
int u() { return d::call(a::S{}) + a::f(a::S{}); }
