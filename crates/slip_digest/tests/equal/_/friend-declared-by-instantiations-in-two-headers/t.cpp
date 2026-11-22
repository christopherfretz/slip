#include "side.h"
// digest §2.2, WP-032 rule 3: `j` is declared only by the instantiations
// of two class templates from two headers; the canonical candidate does not
// depend on the order of the instantiations.
#include "a.h"
#include "c.h"
#ifdef B
C<int> c;
A<int> a;
#else
A<int> a;
C<int> c;
#endif
