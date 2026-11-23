namespace n {
void f(int);
template <class T> class A { friend void f(int) {} T x{}; };
}
n::A<int> a;
void h() { n::f(1); }
