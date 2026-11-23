#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `sizeof` in an array bound and in a constant: the values are written where they are used.
#ifdef B
#pragma pack(push, 1)
#endif
struct S { char c; int i; };
#ifdef B
#pragma pack(pop)
#endif
constexpr unsigned long n = sizeof(S);
int arr[sizeof(S)];
