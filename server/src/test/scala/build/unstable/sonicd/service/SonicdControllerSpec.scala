package build.unstable.sonicd.service

import java.net.InetAddress

import akka.actor.{ActorSystem, Props}
import akka.testkit.{CallingThreadDispatcher, ImplicitSender, TestActorRef, TestKit}
import akka.util.Timeout
import build.unstable.sonic.model.AuthConfig.Mode
import build.unstable.sonic.model._
import build.unstable.sonicd.auth.ApiKey
import build.unstable.sonicd.model.Fixture
import build.unstable.sonicd.system.actor.{AuthenticationActor, SonicdController}
import com.auth0.jwt.JWTSigner
import org.scalatest.{BeforeAndAfterAll, Matchers, WordSpecLike}
import spray.json._

import scala.concurrent.Future
import scala.concurrent.duration._
import scala.util.Failure

class SonicdControllerSpec(_system: ActorSystem) extends TestKit(_system)
  with WordSpecLike with Matchers with BeforeAndAfterAll with ImplicitSender {

  def this() = this(ActorSystem("SonicControllerSpec"))

  override protected def afterAll(): Unit = {
    TestKit.shutdownActorSystem(system)
  }

  def newActor: TestActorRef[SonicdController] =
    TestActorRef[SonicdController](Props(classOf[SonicdController], self, 1.seconds: Timeout)
      .withDispatcher(CallingThreadDispatcher.Id))

  val signer = new JWTSigner("secret")

  "SonicController" should {
    "handle new queries and monitor handler" in {
      val c = newActor

      c ! NewCommand(Fixture.syntheticQuery, None)
      expectMsgType[Props]

      c.underlyingActor.handled shouldBe 1
      // feature removed: c.underlyingActor.handlers(1) shouldBe self.path
    }

    "handle new auth cmds" in {
      val c = newActor

      c ! NewCommand(Authenticate("","", None), None)
      expectMsgType[Authenticate]
    }

    "authorize queries with external auth providers " in {
      val c = newActor
      val config = """{"class" : "SyntheticSource", "security" : 1}""".parseJson.asJsObject
      val auth = new AuthConfig {
        override val provider: Class[_] = classOf[AlwaysSuccessProvider]
        override val config: Map[String, JsValue] = Map.empty
      }
      val syntheticQuery = Query("10", config, Some(auth)).copy(trace_id = Some("1234"))

      c ! NewCommand(syntheticQuery, None)

      expectMsgType[Props]

      c.underlyingActor.handled shouldBe 1
    }

    "reject queries with external auth providers that reject auth" in {
      val c = newActor
      val config = """{"class" : "SyntheticSource", "security" : 1}""".parseJson.asJsObject
      val auth = new AuthConfig {
        val provider: Class[_] = classOf[AlwaysFailsProvider]
        val config: Map[String, JsValue] = Map.empty
      }
      val syntheticQuery = Query("10", config, Some(auth)).copy(trace_id = Some("1234"))

      c ! NewCommand(syntheticQuery, None)

      val done = expectMsgType[Failure[_]]
    }

    "authorize queries on sources without security and users without a known address when user's allowedIps is empty" in {
      val c = newActor
      val config = """{"class" : "SyntheticSource"}""".parseJson.asJsObject
      val claims = ApiKey("1", Mode.Read, 1, None, None).toJWTClaims("bandit")
      val user = AuthenticationActor.fromJWTClaims(claims)
      val auth = SonicdAuth(signer.sign(claims))
      val syntheticQuery = Query("10", config, Some(auth)).copy(trace_id = Some("1234"))

      c ! NewCommand(syntheticQuery, None)
      val cmd = expectMsgType[ValidateToken]
      assert(cmd.token == auth.token)

      lastSender ! user
      expectMsgType[Props]

      c.underlyingActor.handled shouldBe 1
    }

    "authorize queries on sources with security" in {
      val c = newActor
      val config = """{"class" : "SyntheticSource", "security" : 1}""".parseJson.asJsObject
      val claims = ApiKey("1", Mode.Read, 1, None, None).toJWTClaims("bandit")
      val user = AuthenticationActor.fromJWTClaims(claims)
      val auth = SonicdAuth(signer.sign(claims))
      val syntheticQuery = Query("10", config, Some(auth)).copy(trace_id = Some("1234"))

      c ! NewCommand(syntheticQuery, None)
      val cmd = expectMsgType[ValidateToken]
      assert(cmd.token == auth.token)

      lastSender ! user
      expectMsgType[Props]

      c.underlyingActor.handled shouldBe 1

    }

    "reject queries on sources with security that are either unauthenticated or don't have enought authorization" in {
      {
        val c = newActor
        val config = """{"class" : "SyntheticSource", "security" : 2}""".parseJson.asJsObject
        val claims = ApiKey("1", Mode.Read, 1, None, None).toJWTClaims("bandit")
        val user = AuthenticationActor.fromJWTClaims(claims)
        val auth = SonicdAuth(signer.sign(claims))
        val syntheticQuery = Query("10", config, Some(auth)).copy(trace_id = Some("1234"))

        c ! NewCommand(syntheticQuery, None)
        val cmd = expectMsgType[ValidateToken]
        assert(cmd.token == auth.token)

        lastSender ! user
        val done = expectMsgType[Failure[_]]

        done.exception.isInstanceOf[AuthenticationActor.AuthenticationException]

        c.underlyingActor.handled shouldBe 1
        // feature removed: assert(c.underlyingActor.handlers.isEmpty)
      }

      {
        val c = newActor
        val config = """{"class" : "SyntheticSource", "security" : 2}""".parseJson.asJsObject
        val syntheticQuery = Query("10", config, None).copy(trace_id = Some("1234"))

        c ! NewCommand(syntheticQuery, None)

        val done = expectMsgType[Failure[_]]

        done.exception.isInstanceOf[AuthenticationActor.AuthenticationException]

        c.underlyingActor.handled shouldBe 1
        // feature removed: assert(c.underlyingActor.handlers.isEmpty)
      }
    }

    "reject queries on sources with ip-blocking enabled from clients that are not in whitelist" in {
      val c = newActor
      val config = """{"class" : "SyntheticSource", "security" : 1}""".parseJson.asJsObject
      val allowedIps = InetAddress.getByName("10.0.0.1") :: Nil
      val claims = ApiKey("1", Mode.Read, 1, Some(allowedIps), None).toJWTClaims("bandit")
      val user = AuthenticationActor.fromJWTClaims(claims)
      val auth = SonicdAuth(signer.sign(claims))
      val syntheticQuery = Query("10", config, Some(auth)).copy(trace_id = Some("1234"))

      c ! NewCommand(syntheticQuery, Some(InetAddress.getByName("localhost")))
      val cmd = expectMsgType[ValidateToken]
      assert(cmd.token == auth.token)

      lastSender ! user
      val done = expectMsgType[Failure[_]]

      done.exception.isInstanceOf[AuthenticationActor.AuthenticationException]

      c.underlyingActor.handled shouldBe 1
      // feature removed: assert(c.underlyingActor.handlers.isEmpty)
    }

    "accept queries on sources with ip-blocking enabled from clients that are whitelisted" in {
      val c = newActor
      val config = """{"class" : "SyntheticSource", "security" : 1}""".parseJson.asJsObject
      val allowedIps = InetAddress.getByName("localhost") :: Nil
      val claims = ApiKey("1", Mode.Read, 1, Some(allowedIps), None).toJWTClaims("bandit")
      val user = AuthenticationActor.fromJWTClaims(claims)
      val auth = SonicdAuth(signer.sign(claims))
      val syntheticQuery = Query("10", config, Some(auth)).copy(trace_id = Some("1234"))

      c ! NewCommand(syntheticQuery, Some(allowedIps.head))
      val cmd = expectMsgType[ValidateToken]
      assert(cmd.token == auth.token)

      lastSender ! user
      expectMsgType[Props]

      c.underlyingActor.handled shouldBe 1
    }
  }
}

class AlwaysSuccessProvider extends ExternalAuthProvider {

  override def validate(auth: AuthConfig, system: ActorSystem, traceId: String): Future[ApiUser] =
    Future.successful(ApiUser("", 10, AuthConfig.Mode.ReadWrite, None))
}

class AlwaysFailsProvider extends ExternalAuthProvider {

  override def validate(auth: AuthConfig, system: ActorSystem, traceId: String): Future[ApiUser] =
    Future.failed(new Exception("not a chance!"))
}

