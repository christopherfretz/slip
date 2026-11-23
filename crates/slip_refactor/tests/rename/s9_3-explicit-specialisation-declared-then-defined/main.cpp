namespace n {
template <class T> void f(T) {}
template <> void f<int>(int);
template <> void f<int>(int) {}
}
void h() { n::f(1); }
