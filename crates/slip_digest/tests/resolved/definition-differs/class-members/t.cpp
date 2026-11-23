#include "side.h"
// digest-kinds §11.9 row `RecordData` `members`. The list holds the
// members that are declared units (§5.5); the members of an implicit
// instantiation are derived, so only an explicitly specialised member
// is listed in `definition`. A field added to the pattern shows in the
// pattern and in the derived field, not in this row.
template <class T> struct X { int a; void f(); };
X<int> x;
#ifdef B
template <> void X<int>::f() {}
#endif
