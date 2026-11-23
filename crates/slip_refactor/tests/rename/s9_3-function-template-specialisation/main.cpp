namespace n {
template <class T> void f(T);
template <class T> void f(T) {}
template <> void f<int>(int) {}
template <> void f(char) {}
}
void h() { n::f<int>(1); n::f('c'); n::f(2.0); }
