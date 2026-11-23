namespace n {
void f(int);
template <class T> void t(T x) { f(x); }
}
namespace q { struct Q {}; void f(Q); }
void h() { n::t(q::Q()); }
