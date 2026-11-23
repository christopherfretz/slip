// std: c++20
#include "side.h"
// digest-kinds §11.1 property 5 (WP-046 review F2): one destroying
// operator delete, reached through another base path, so the `this`
// argument converted to its parameter differs and the operator does not.
namespace std {
struct destroying_delete_t { explicit destroying_delete_t() = default; };
}
struct B0 { void operator delete(B0 *, std::destroying_delete_t); };
struct M : B0 {};
#ifdef B
struct D : M { virtual ~D(); };
#else
struct D : B0 { virtual ~D(); };
#endif
D::~D() {}
