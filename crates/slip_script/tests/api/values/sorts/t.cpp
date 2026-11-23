namespace n {
struct S;
struct S {};
using A = S;
int f(int a);
int v = 1;
template <class X> struct T {};
}
