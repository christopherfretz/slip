namespace n {
void f(int);
template <class T> void t(T x) { f(x); x.f(); }
}
void h() { n::f(1); }
