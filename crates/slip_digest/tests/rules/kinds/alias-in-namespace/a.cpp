namespace acme {
struct S {};
typedef S T;
using U = S *;
}
acme::U use(acme::T *t) { return t; }
