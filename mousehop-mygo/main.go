package main

import "github.com/alvin-7/mousehop/mygo/internal/app"

var version = "development"
var commit string
var applicationName = "Mousehop"

func main() { app.Run(version, commit, applicationName) }
