#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a pointer to member is the member and its path, no offset.
#ifdef B
#pragma pack(push, 1)
#endif
struct S { char c; int i; };
#ifdef B
#pragma pack(pop)
#endif
constexpr int S::*pm = &S::i;
int g(S &s) { return s.*pm; }
