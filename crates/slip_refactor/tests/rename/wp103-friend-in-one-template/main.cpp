namespace n {
void f(int);
template <class T> class A { friend void f(int); T x{}; };
void f(int) {}
}
n::A<int> a;
void h() { n::f(1); }
