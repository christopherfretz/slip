#include "side.h"
// §4.4 "Spellings", rule 11: a qualified pseudo-destructor in a generic lambda in a function that is no template: the body is templated (`own_spelling::templated_stream`), the qualifier holds the found set of `f` for the lambda's parameter, and Kiln keeps that the name was qualified, not the qualifier, so the unit is indeterminate (`spelling/pseudo-destructor-qualifier`) rather than equal.
template<int N> struct Bq { using I = int; };
struct X {};
int f(int); int h(X);
#ifdef B
void k() { auto l = [](auto t, int *p) { p->Bq<sizeof(sizeof(h(t)))>::I::~I(); }; (void)l; }
#else
void k() { auto l = [](auto t, int *p) { p->Bq<sizeof(sizeof(f(t)))>::I::~I(); }; (void)l; }
#endif
