namespace n {
template <class U> struct S { void f() {} };
template <class T> void t(T x) { x.f(); }
}
void h() { n::S<int> s; n::t(s); }
