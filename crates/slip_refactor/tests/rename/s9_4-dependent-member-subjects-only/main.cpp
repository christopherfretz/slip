namespace n {
struct S { void f(); static void f(int); };
struct R { void f(); static void f(int); };
template <class T> void t(T x, T* p) { T::f(1); x.f(); p->f(); }
}
void h() { n::S s; n::t(s, &s); }
