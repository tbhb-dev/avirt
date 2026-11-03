// SPDX-License-Identifier: Apache-2.0
// Copyright Tony Burns

// Command avirt is the avirt command-line tool.
package main

import (
	"fmt"
	"io"
	"os"

	"github.com/spf13/cobra"

	"go.tbhb.dev/avirt/internal/buildmeta"
)

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}

// run executes the root command and returns the process exit code.
func run(args []string, stdout, stderr io.Writer) int {
	cmd := newRootCmd()
	cmd.SetArgs(args)
	cmd.SetOut(stdout)
	cmd.SetErr(stderr)

	err := cmd.Execute()
	if err != nil {
		return 1
	}

	return 0
}

func versionString() string {
	info := buildmeta.Get()

	return fmt.Sprintf("%s (commit %s, date %s)", info.Version, info.Commit, info.Date)
}

func newRootCmd() *cobra.Command {
	root := &cobra.Command{
		Use:          "avirt",
		Short:        "avirt command-line tool",
		Version:      versionString(),
		SilenceUsage: true,
	}
	root.SetVersionTemplate("avirt {{.Version}}\n")
	root.AddCommand(newVersionCmd())

	return root
}

func newVersionCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "version",
		Short: "Print version, commit and date",
		Args:  cobra.NoArgs,
		Run: func(cmd *cobra.Command, _ []string) {
			fmt.Fprintf(cmd.OutOrStdout(), "avirt %s\n", versionString())
		},
	}
}
