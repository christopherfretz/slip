#pragma redefine_extname g impl_f
extern "C" int g(int);

int caller() { return g(1); }
