import com.typesafe.sbt.SbtGit.GitKeys._
import sbt.Keys._
import sbt._
import sbtassembly.AssemblyKeys._
import sbtassembly.{MergeStrategy, PathList}
import sbtbuildinfo.BuildInfoKeys._
import sbtbuildinfo.BuildInfoPlugin
import sbtbuildinfo.BuildInfoPlugin._
import spray.revolver.RevolverPlugin._

object Sonic extends Build {

  val scalaV = "2.11.7"
  val akkaV = "2.4.4"
  val sonicdV = "0.3.2"
  val sparkV = "1.6.1"

  val commonSettings = Seq(
    organization := "build.unstable",
    version := sonicdV,
    scalaVersion := scalaV,
    licenses +=("MIT", url("https://opensource.org/licenses/MIT")),
    publishMavenStyle := false,
    scalacOptions := Seq(
      "-unchecked",
      "-Xlog-free-terms",
      "-deprecation",
      "-encoding", "UTF-8",
      "-target:jvm-1.8"
    )
  )

  val meta = """META.INF(.)*""".r

  val assemblyStrategy = assemblyMergeStrategy in assembly := {
    case PathList(ps@_*) if ps.last endsWith ".class" => MergeStrategy.last //FIXME dangerous!
    case PathList(ps@_*) if ps.last endsWith ".jar" => MergeStrategy.last
    case PathList("javax", "servlet", xs@_*) => MergeStrategy.last
    case "reference.conf" => MergeStrategy.concat
    case meta(_) => MergeStrategy.discard
    case x =>
      val oldStrategy = (assemblyMergeStrategy in assembly).value
      oldStrategy(x)
  }

  val core: Project = Project("sonicd-core", file("lib/scala"))
    .settings(commonSettings: _*)
    .settings(
      libraryDependencies ++= {
        Seq(
          "io.spray" %% "spray-json" % "1.3.2",
          "com.typesafe.akka" %% "akka-actor" % akkaV,
          "com.typesafe.akka" %% "akka-slf4j" % akkaV,
          "com.typesafe.akka" %% "akka-stream" % akkaV,
          "com.typesafe.akka" %% "akka-http-spray-json-experimental" % akkaV,
          "org.scalatest" %% "scalatest" % "2.2.5" % "test"
        )
      }
    )

  val spark: Project = Project("sonicd-spark", file("server/spark"))
    .settings(commonSettings: _*)
    .settings(
      assemblyStrategy,
      libraryDependencies ++= {
        Seq(
          "org.apache.spark" %% "spark-sql" % sparkV excludeAll ExclusionRule(name = "slf4j-log4j12"),
          "ch.qos.logback" % "logback-classic" % "1.0.13"
        )
      },
      artifact in(Compile, assembly) := {
        val art = (artifact in(Compile, assembly)).value
        art.copy(`classifier` = Some("assembly"))
      }
    ).settings(addArtifact(artifact in(Compile, assembly), assembly).settings: _*)
    .dependsOn(core)

  val AsResource = config("asResource")

  val server: Project = Project("sonicd-server", file("server"))
    .settings(Revolver.settings: _*)
    .settings(commonSettings: _*)
    .settings(net.virtualvoid.sbt.graph.Plugin.graphSettings: _*)
    .enablePlugins(BuildInfoPlugin)
    .enablePlugins(com.typesafe.sbt.GitVersioning)
    .settings(
      buildInfoKeys ++= Seq[BuildInfoKey](
        version,
        "builtAt" -> {
          val dtf = new java.text.SimpleDateFormat("yyyy-MM-dd HH:mm:ssZ")
          dtf.setTimeZone(java.util.TimeZone.getTimeZone("America/Los_Angeles"))
          dtf.format(new java.util.Date())
        },
        "commit" -> gitHeadCommit.value.map(_.take(7)).getOrElse("unknown-commit")),
      buildInfoPackage := "build.unstable.sonicd",
      ivyConfigurations += AsResource,
      resources in Compile ++= update.value.select(configurationFilter(AsResource.name)),
      assemblyStrategy,
      assemblyJarName in assembly := "sonicd-assembly.jar",
      libraryDependencies ++= {
        Seq(
          //spark source
          "build.unstable" %% "sonicd-spark" % sonicdV % AsResource classifier "assembly" notTransitive(),
          "org.apache.spark" %% "spark-core" % sparkV excludeAll ExclusionRule(name = "slf4j-log4j12"),
          "org.apache.spark" %% "spark-yarn" % sparkV excludeAll ExclusionRule(name = "slf4j-log4j12"),
          "org.apache.spark" %% "spark-sql" % sparkV excludeAll ExclusionRule(name = "slf4j-log4j12"),
          //core
          "com.typesafe.akka" %% "akka-http-core" % akkaV,
          "ch.qos.logback" % "logback-classic" % "1.0.13",
          "com.typesafe.akka" %% "akka-http-testkit" % akkaV % "test",
          "com.h2database" % "h2" % "1.3.175" % "test"
        )
      }
    )
    .dependsOn(core % "compile->compile;test->test")

  val examples = Project("sonicd-examples", file("examples/scala"))
    .settings(Revolver.settings: _*)
    .settings(commonSettings: _*)
    .settings(
      libraryDependencies ++= {
        Seq(
          "ch.qos.logback" % "logback-classic" % "1.0.13"
        )
      }
    ).dependsOn(core)
}