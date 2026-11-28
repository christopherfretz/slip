#include "side.h"
// §4.4 "Spellings", rule 5 (WP-138): `spelling/decltype`, a fold expression in
// an enumeration's canonical fixed underlying type has no spelling of its own
// and raises `oper_left_out` in profile mode.
namespace N { struct X {}; }
N::X operator+(N::X, N::X);
template<class... T> struct E { enum class K : decltype((T{} + ...)) {}; };
