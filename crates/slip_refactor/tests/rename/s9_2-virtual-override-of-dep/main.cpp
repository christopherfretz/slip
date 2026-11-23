#include "lib.h"
namespace n {
struct D : lib::B { void f() override; };
}
void h(n::D& d) { d.f(); }
