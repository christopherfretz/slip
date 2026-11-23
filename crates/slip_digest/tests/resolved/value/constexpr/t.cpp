#include "side.h"
// digest-kinds §11.3, a constant: Kiln evaluates a constexpr variable at
// its declaration (kiln_sema decl.rs check_constexpr_init), so the
// resolved unit is on both sides and a use changes nothing.
constexpr int x = 1;
#ifdef B
int h() { int a[x]; return sizeof a; }
#endif
