#include "side.h"
// WP-035, digest-kinds §7 `_` `pseudo-destructor-qualified-outside-template`: `p->~I()` against `p->I::~I()` outside a template: only substitution reads the qualifier.
typedef int I;
I *p;
#ifdef B
int v = (p->I::~I(), 0);
#else
int v = (p->~I(), 0);
#endif
