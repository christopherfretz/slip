namespace n { void f(int); }
namespace m {
void f(int);
template <class T> class A { friend void f(int); T x{}; };
}
m::A<int> a;
void h() { n::f(1); m::f(2); }
