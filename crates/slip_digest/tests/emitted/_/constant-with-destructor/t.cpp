#include "side.h"
// WP-155: a variable whose initialiser is a constant but whose type has a
// destructor that is not trivial is emitted: its destructor is registered
// to run at exit. Through an array too.
struct D { constexpr D() {} ~D() {} };
constinit D d;
D ds[2];
