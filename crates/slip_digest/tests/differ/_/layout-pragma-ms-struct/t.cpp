#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `#pragma ms_struct on` gives the class an implicit `ms_struct` (kiln_sema `ms_struct.rs:30`).
#ifdef B
#pragma ms_struct on
struct S { char a : 4; int b : 4; };
#pragma ms_struct off
#else
struct S { char a : 4; int b : 4; };
#endif
