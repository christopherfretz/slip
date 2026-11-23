// std: c++26
#include "side.h"
// digest §4.3, §4.4 back-references, `backref-content-unit`: a content-keyed unit (a consteval block of a class, whose key is its digest, computed inside the class's stream) holds nodes reached twice; its locals renamed give the same digests.
#ifdef B
struct S { consteval { int k = 1; template for (int e : {1, 2, 3}) { (void)(e + k); } } };
#else
struct S { consteval { int n = 1; template for (int x : {1, 2, 3}) { (void)(x + n); } } };
#endif
