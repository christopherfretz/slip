// std: c++20
// §5.3 DeclMisc `0-module-ownership`: an exported declaration against one that is not.
export module m;
#ifdef B
export int x;
#else
int x;
#endif
