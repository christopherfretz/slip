template <class T> struct W1 { T a; };
template <class T> struct W2 { T *b; };
template <class> struct W3 {};
struct A {};
W3<A> w3;
void g(W2<A> *);
static_assert(sizeof(W1<A>) == sizeof(A));
