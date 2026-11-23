namespace n {
void f(int);
template <class T> class A { friend void f(int); T x{}; };
template <class T> class B { friend void f(int); T y{}; };
void f(int) {}
}
n::A<int> a;
n::B<long> b;
void h() { n::f(1); }
