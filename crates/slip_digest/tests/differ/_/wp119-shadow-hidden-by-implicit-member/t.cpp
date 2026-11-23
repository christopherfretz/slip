#include "side.h"
// spec/digest.md §2.1 (WP-119 item 6): a shadow hidden by an implicit member (D's copy assignment operator, declared on use, hides Base::operator=(const D&), [namespace.udecl]/14) stays a unit: side b uses the assignment; D does not differ and the shadow is not removed (§1 rule 5). Kills the mutation "an implicit hider counts".
struct D;
struct Base { Base& operator=(const D&); };
struct D : Base { using Base::operator=; };
#ifdef B
void u(D& a, const D& b) { a = b; }
#endif
