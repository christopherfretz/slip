#include "side.h"
// spec/digest.md §2.1 (WP-119 item 6): a shadow hidden by an implicit member (D's copy assignment operator, declared on use, hides Base::operator=(const D&), [namespace.udecl]/14) stays a unit: side b uses the assignment; D does not differ and the shadow is not removed (§1 rule 5). (Killed the mutation "an implicit hider counts" until WP-163: Kiln marks the shadow in both sides since WP-3145/WP-3146.)
struct D;
struct Base { Base& operator=(const D&); };
struct D : Base { using Base::operator=; };
#ifdef B
void u(D& a, const D& b) { a = b; }
#endif
