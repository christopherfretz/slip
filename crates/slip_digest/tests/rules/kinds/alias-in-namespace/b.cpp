namespace legacy {
struct S {};
typedef S T;
using U = S *;
}
legacy::U use(legacy::T *t) { return t; }
