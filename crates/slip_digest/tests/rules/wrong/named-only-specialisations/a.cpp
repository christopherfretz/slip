template <class> struct W1;
template <class> struct W2;
template <class> struct W3 {};
struct A {};
W3<A> w3;
void f(W1<A> *);
void g(W1<A> *);
